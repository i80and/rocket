use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use page::Page;
use toctree::TocTree;
use handlebars::{self, Handlebars};
use serde_json;
use toml;

#[derive(Deserialize)]
struct RawConfig {
    templates: HashMap<String, PathBuf>,
    constants: Option<serde_json::map::Map<String, serde_json::Value>>,
}

pub struct Theme {
    handlebars: Handlebars,
    constants: serde_json::map::Map<String, serde_json::Value>,
}

impl Theme {
    pub fn load(path: &Path) -> Result<Self, ()> {
        let mut handlebars = Handlebars::new();
        let theme_dir_path = path.parent().unwrap_or_else(|| Path::new(""));

        let mut file = File::open(path).or(Err(()))?;
        let mut data = String::new();
        file.read_to_string(&mut data).or(Err(()))?;
        let config: RawConfig = toml::from_str(&data).or(Err(()))?;

        for (template_name, template_path) in &config.templates {
            let template_path = theme_dir_path.join(template_path);
            handlebars
                .register_template_file(template_name, template_path)
                .ok()
                .unwrap();
        }

        let constants = config.constants.unwrap_or_else(serde_json::map::Map::new);
        Ok(Theme {
               handlebars: handlebars,
               constants: constants,
           })
    }

    pub fn render(&self,
                  template_name: &str,
                  project_args: &serde_json::map::Map<String, serde_json::Value>,
                  page: &Page,
                  toctree: &TocTree)
                  -> Result<String, handlebars::RenderError> {
        let ctx = json!({
            "page": &page.theme_config,
            "project": project_args,
            "theme": self.constants,
            "body": &page.body,
            "toc": toctree.generate_html(""),
        });
        self.handlebars.render(template_name, &ctx)
    }
}
