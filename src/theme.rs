use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use page::{Page, Slug};
use toctree::TocTree;
use handlebars::{self, Handlebars};
use regex::Regex;
use serde_json;
use toml;

lazy_static! {
    static ref PAT_TAGS: Regex = Regex::new("<[^>]+>").expect("Failed to compile striptags regex");
}

struct TocTreeHelper {
    toctree: Arc<TocTree>,
    current_slug: Slug,
}

impl handlebars::HelperDef for TocTreeHelper {
    fn call(
        &self,
        h: &handlebars::Helper,
        _: &Handlebars,
        rc: &mut handlebars::RenderContext,
    ) -> Result<(), handlebars::RenderError> {
        let slug = h.param(0).unwrap().value().as_str().unwrap();
        let html = self.toctree
            .generate_html(&Slug::new(slug.to_owned()), &self.current_slug, true)
            .or_else(|msg| Err(handlebars::RenderError::new(msg)))?
            .concat();
        rc.writer.write_all(html.as_bytes())?;
        Ok(())
    }
}

struct StripTags;

impl handlebars::HelperDef for StripTags {
    fn call(
        &self,
        h: &handlebars::Helper,
        _: &Handlebars,
        rc: &mut handlebars::RenderContext,
    ) -> Result<(), handlebars::RenderError> {
        let arg = h.param(0).unwrap().value().as_str().unwrap();
        let stripped = PAT_TAGS.replace_all(arg, "");
        rc.writer.write_all(stripped.as_bytes())?;
        Ok(())
    }
}

#[derive(Deserialize)]
struct RawConfig {
    constants: Option<serde_json::map::Map<String, serde_json::Value>>,
    templates: HashMap<String, PathBuf>,
}

pub struct Theme {
    path: PathBuf,
    constants: serde_json::map::Map<String, serde_json::Value>,
    templates: HashMap<String, PathBuf>,
}

impl Theme {
    pub fn load(path: &Path) -> Result<Self, ()> {
        let mut file = File::open(path).or(Err(()))?;
        let mut data = String::new();
        file.read_to_string(&mut data).or(Err(()))?;
        let config: RawConfig = toml::from_str(&data).or(Err(()))?;

        let constants = config.constants.unwrap_or_else(serde_json::map::Map::new);
        Ok(Theme {
            path: path.to_owned(),
            constants: constants,
            templates: config.templates,
        })
    }
}

pub struct Renderer<'a> {
    toctree: Arc<TocTree>,
    handlebars: Handlebars,
    constants: &'a serde_json::map::Map<String, serde_json::Value>,
}

impl<'a> Renderer<'a> {
    pub fn new(
        theme: &'a Theme,
        toctree: TocTree,
    ) -> Result<Renderer<'a>, handlebars::TemplateFileError> {
        let mut handlebars = Handlebars::new();
        let theme_dir_path = theme.path.parent().unwrap_or_else(|| Path::new(""));

        for (template_name, template_path) in &theme.templates {
            let template_path = theme_dir_path.join(template_path);
            handlebars.register_template_file(template_name, template_path)?;
        }

        handlebars.register_helper("striptags", Box::new(StripTags));

        Ok(Renderer {
            toctree: Arc::new(toctree),
            handlebars,
            constants: &theme.constants,
        })
    }

    pub fn render(
        &mut self,
        template_name: &str,
        project_args: &serde_json::map::Map<String, serde_json::Value>,
        page: &Page,
        body: &str,
    ) -> Result<String, handlebars::RenderError> {
        let ctx = json!({
            "page": &page.theme_config,
            "project": project_args,
            "theme": self.constants,
            "body": body,
        });

        let helper = TocTreeHelper {
            toctree: Arc::clone(&self.toctree),
            current_slug: page.slug.to_owned(),
        };
        self.handlebars.register_helper("toctree", Box::new(helper));

        self.handlebars.render(template_name, &ctx)
    }
}
