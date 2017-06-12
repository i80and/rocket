extern crate argparse;
extern crate comrak;
extern crate lazycell;
#[macro_use]
extern crate lazy_static;
extern crate liquid;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
extern crate simple_logger;
extern crate syntect;
extern crate regex;
extern crate time;
extern crate toml;
extern crate typed_arena;

mod directives;
mod evaluator;
mod highlighter;
mod lex;
mod markdown;
mod parse;
mod util;

use std::fs::{self, File};
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use argparse::{ArgumentParser, StoreTrue};
use evaluator::Evaluator;

#[derive(Deserialize)]
struct ConfigFile {
    syntax_theme: Option<String>,
    theme: Option<PathBuf>,
    content_dir: Option<PathBuf>,
    output: Option<PathBuf>,
}

struct ConfigOptions {
    verbose: bool,
    syntax_theme: String,
    theme: PathBuf,
    content_dir: PathBuf,
    output: PathBuf,
}

impl ConfigOptions {
    fn read_toml(path: &Path) -> Result<ConfigOptions, ()> {
        let mut file = File::open(path).or(Err(()))?;
        let mut data = String::new();
        file.read_to_string(&mut data).or(Err(()))?;

        let config: ConfigFile = toml::from_str(&data).or(Err(()))?;
        Ok(ConfigOptions {
               verbose: false,
               syntax_theme: config
                   .syntax_theme
                   .unwrap_or_else(|| highlighter::DEFAULT_SYNTAX_THEME.to_owned()),
               theme: config.theme.unwrap_or_else(|| PathBuf::from("theme")),
               content_dir: config
                   .content_dir
                   .unwrap_or_else(|| PathBuf::from("content")),
               output: config.output.unwrap_or_else(|| PathBuf::from("build")),
           })
    }

    fn build_file(&self, evaluator: &Evaluator, path: &Path) -> Result<(), ()> {
        let node = match evaluator.parser.borrow_mut().parse(path) {
            Ok(n) => n,
            Err(_) => {
                error!("Failed to parse {}", path.to_string_lossy());
                return Err(());
            }
        };
        let output = evaluator.evaluate(&node);

        let path_from_content_root = path.strip_prefix(&self.content_dir)
            .expect("Failed to get output path");
        let mut output_path = self.output.join(path_from_content_root);
        output_path.set_extension("html");
        let output_dir = output_path.parent().expect("Couldn't get output directory");

        fs::create_dir_all(output_dir).or(Err(()))?;
        let mut file = File::create(&output_path).or(Err(()))?;
        file.write_all(output.as_bytes()).or(Err(()))?;

        Ok(())
    }

    fn build_project(&self) {
        let mut evaluator = Evaluator::new_with_options(&self.syntax_theme);
        evaluator.register("md", Box::new(directives::Markdown::new()));
        evaluator.register("table", Box::new(directives::Dummy::new()));
        evaluator.register("version", Box::new(directives::Version::new("3.4.0")));
        evaluator.register("note",
                           Box::new(directives::Admonition::new("Note", "note")));
        evaluator.register("warning",
                           Box::new(directives::Admonition::new("Warning", "warning")));
        evaluator.register("manual",
                           Box::new(directives::LinkTemplate::new("https://docs.mongodb.com/manual")));
        evaluator.register("definition-list",
                           Box::new(directives::DefinitionList::new()));
        evaluator.register("concat", Box::new(directives::Concat::new()));
        evaluator.register("include", Box::new(directives::Include::new()));
        evaluator.register("null", Box::new(directives::Dummy::new()));
        evaluator.register("let", Box::new(directives::Let::new()));

        util::visit_dirs(self.content_dir.as_ref(),
                         &|path| match self.build_file(&evaluator, path) {
                             Ok(_) => (),
                             Err(_) => {
                                 error!("Failed to build {}", path.to_string_lossy());
                             }
                         })
                .expect("Error crawling content tree");
    }
}

fn main() {
    let mut config = ConfigOptions::read_toml(Path::new("config.toml"))
        .expect("Failed to open config.toml");

    {
        let mut ap = ArgumentParser::new();
        ap.set_description("The Rocket documentation build system.");
        ap.refer(&mut config.verbose)
            .add_option(&["-v", "--verbose"], StoreTrue, "Be verbose");
        ap.parse_args_or_exit();
    }

    let loglevel = if config.verbose {
        log::LogLevel::Debug
    } else {
        log::LogLevel::Info
    };

    simple_logger::init_with_level(loglevel).expect("Failed to initialize logger");

    let start_time = time::precise_time_ns();
    config.build_project();

    info!("Took {} seconds",
          (time::precise_time_ns() - start_time) as f64 / (1_000_000_000 as f64));
}
