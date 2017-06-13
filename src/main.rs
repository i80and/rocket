extern crate argparse;
extern crate comrak;
extern crate glob;
extern crate handlebars;
extern crate lazycell;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
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
mod theme;
mod util;

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use argparse::{ArgumentParser, StoreTrue};
use evaluator::Evaluator;

#[derive(Deserialize)]
struct RawConfig {
    syntax_theme: Option<String>,
    theme: Option<PathBuf>,
    content_dir: Option<PathBuf>,
    output: Option<PathBuf>,
    templates: HashMap<String, String>,
}

struct Project {
    verbose: bool,
    syntax_theme: String,
    theme: theme::Theme,
    content_dir: PathBuf,
    output: PathBuf,
    templates: Vec<(glob::Pattern, String)>,
}

impl Project {
    fn read_toml(path: &Path) -> Result<Project, ()> {
        let mut file = File::open(path).or(Err(()))?;
        let mut data = String::new();
        file.read_to_string(&mut data).or(Err(()))?;
        let config: RawConfig = toml::from_str(&data).or(Err(()))?;

        let theme_path = config.theme.ok_or(())?;
        let theme = theme::Theme::load(&theme_path)?;

        let path_patterns: Result<Vec<_>, ()> = config
            .templates
            .iter()
            .map(|(k, v)| {
                     let pattern = match glob::Pattern::new(k) {
                         Ok(p) => p,
                         Err(_) => return Err(()),
                     };
                     Ok((pattern, v.to_owned()))
                 })
            .collect();

        let path_patterns = path_patterns.or(Err(()))?;

        Ok(Project {
               verbose: false,
               syntax_theme: config
                   .syntax_theme
                   .unwrap_or_else(|| highlighter::DEFAULT_SYNTAX_THEME.to_owned()),
               theme: theme,
               content_dir: config
                   .content_dir
                   .unwrap_or_else(|| PathBuf::from("content")),
               output: config.output.unwrap_or_else(|| PathBuf::from("build")),
               templates: path_patterns,
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

        // Find the template that matches this path
        let template_name = self.templates
            .iter()
            .find(|&&(ref pat, _)| pat.matches_path(path))
            .map(|&(_, ref name)| name.as_ref())
            .unwrap_or("default");

        let rendered = self.theme.render(template_name, &output).or(Err(()))?;

        let path_from_content_root = path.strip_prefix(&self.content_dir)
            .expect("Failed to get output path");
        let mut output_path = self.output.join(path_from_content_root);
        output_path.set_extension("html");
        let output_dir = output_path.parent().expect("Couldn't get output directory");

        fs::create_dir_all(output_dir).or(Err(()))?;
        let mut file = File::create(&output_path).or(Err(()))?;
        file.write_all(rendered.as_bytes()).or(Err(()))?;

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
    let mut config = Project::read_toml(Path::new("config.toml"))
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
