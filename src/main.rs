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
extern crate walkdir;

mod directives;
mod evaluator;
mod highlighter;
mod lex;
mod markdown;
mod page;
mod parse;
mod theme;
mod toctree;

use std::collections::HashMap;
use std::rc::Rc;
use std::convert::From;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::mem;
use std::path::{Path, PathBuf};
use argparse::{ArgumentParser, StoreTrue};
use evaluator::Evaluator;
use page::{Page, Slug};
use toctree::TocTree;

#[derive(Debug)]
enum LinkError {
    TemplateError(handlebars::RenderError),
    IOError(io::Error),
}

impl From<handlebars::RenderError> for LinkError {
    fn from(orig: handlebars::RenderError) -> Self {
        LinkError::TemplateError(orig)
    }
}

impl From<io::Error> for LinkError {
    fn from(orig: io::Error) -> Self {
        LinkError::IOError(orig)
    }
}

#[derive(Deserialize)]
struct RawConfig {
    syntax_theme: Option<String>,
    theme: Option<PathBuf>,
    content_dir: Option<PathBuf>,
    output: Option<PathBuf>,
    templates: HashMap<String, String>,
    theme_constants: Option<serde_json::map::Map<String, serde_json::Value>>,
}

struct Project {
    verbose: bool,
    theme: theme::Theme,
    content_dir: PathBuf,
    output: PathBuf,
    templates: Vec<(glob::Pattern, String)>,
    theme_constants: serde_json::map::Map<String, serde_json::Value>,
    syntax_theme: String,

    pretty_url: bool,
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

        let syntax_theme = config
            .syntax_theme
            .unwrap_or_else(|| highlighter::DEFAULT_SYNTAX_THEME.to_owned());

        Ok(Project {
                verbose: false,
                theme,
                content_dir: config
                    .content_dir
                    .unwrap_or_else(|| PathBuf::from("content")),
                output: config.output.unwrap_or_else(|| PathBuf::from("build")),
                templates: path_patterns,
                theme_constants: config
                    .theme_constants
                    .unwrap_or_else(serde_json::map::Map::new),
                syntax_theme,
                pretty_url: true,
           })
    }

    fn build_file(&self, evaluator: &mut Evaluator, slug: &Slug, path: &Path) -> Result<Page, ()> {
        debug!("Compiling {}", slug);

        let node = match evaluator.parser.parse(path) {
            Ok(n) => n,
            Err(_) => {
                error!("Failed to parse {}", path.to_string_lossy());
                return Err(());
            }
        };

        let output = evaluator.evaluate(&node);

        let page = Page {
            source_path: path.to_owned(),
            slug: slug.clone(),
            body: output,
            theme_config: evaluator.theme_config.clone(),
        };

        evaluator.reset();
        Ok(page)
    }

    fn link_file(&self, page: &Page, renderer: &mut theme::Renderer) -> Result<(), LinkError> {
        debug!("Linking {}", &page.slug);

        // Find the template that matches this path
        let template_name = self.templates
            .iter()
            .find(|&&(ref pat, _)| pat.matches_path(&page.source_path))
            .map(|&(_, ref name)| name.as_ref())
            .unwrap_or("default");

        let rendered = renderer.render(template_name, &self.theme_constants, page)?;
        let output_path = page.slug.create_output_path(&self.output, self.pretty_url);
        let output_dir = output_path.parent().expect("Couldn't get output directory");

        fs::create_dir_all(output_dir)?;
        let mut file = File::create(&output_path)?;
        file.write_all(rendered.as_bytes())?;

        Ok(())
    }

    fn build_project(&self, evaluator: &mut Evaluator) {
        let mut pending_pages = vec![];
        let mut titles = HashMap::new();

        for entry in walkdir::WalkDir::new(&self.content_dir) {
            let entry = entry.expect("Failed to walk dir");
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let slug = path.strip_prefix(&self.content_dir)
                .expect("Failed to get output path");
            let dir = slug.parent().unwrap();
            let stem = slug.file_stem().unwrap();
            let slug = Slug::new(dir.join(stem).to_string_lossy().as_ref().to_owned());

            evaluator.set_slug(slug.clone());
            match self.build_file(evaluator, &slug, path) {
                Ok(page) => {
                    titles.insert(page.slug.to_owned(), page.title());
                    pending_pages.push(page);
                }
                Err(_) => {
                    error!("Failed to build {}", path.to_string_lossy());
                }
            }
        }

        let mut toctree = mem::replace(&mut evaluator.toctree, TocTree::new_empty());
        toctree.finish(titles);

        let mut renderer = theme::Renderer::new(&self.theme, toctree)
            .expect("Failed to construct renderer");
        for page in &pending_pages {
            self.link_file(page, &mut renderer)
                .expect("Failed to link page");
        }
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

    let mut evaluator = Evaluator::new_with_options(&config.syntax_theme);
    evaluator.register("md", Rc::new(directives::Markdown::new()));
    evaluator.register("table", Rc::new(directives::Dummy::new()));
    evaluator.register("version", Rc::new(directives::Version::new("3.4.0")));
    evaluator.register("note",
                       Rc::new(directives::Admonition::new("Note", "note")));
    evaluator.register("warning",
                       Rc::new(directives::Admonition::new("Warning", "warning")));
    evaluator.register("define-template", Rc::new(directives::DefineTemplate::new()));
    evaluator.register("definition-list",
                       Rc::new(directives::DefinitionList::new()));
    evaluator.register("concat", Rc::new(directives::Concat::new()));
    evaluator.register("include", Rc::new(directives::Include::new()));
    evaluator.register("import", Rc::new(directives::Import::new()));
    evaluator.register("null", Rc::new(directives::Dummy::new()));
    evaluator.register("let", Rc::new(directives::Let::new()));
    evaluator.register("define", Rc::new(directives::Define::new()));
    evaluator.register("get", Rc::new(directives::Get::new()));
    evaluator.register("theme-config", Rc::new(directives::ThemeConfig::new()));
    evaluator.register("toctree", Rc::new(directives::TocTree::new()));

    let start_time = time::precise_time_ns();
    config.build_project(&mut evaluator);

    info!("Took {} seconds",
          (time::precise_time_ns() - start_time) as f64 / (1_000_000_000 as f64));
}
