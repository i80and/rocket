use std::path::PathBuf;
use serde_json::{self, Value};

pub struct Page {
    pub source_path: PathBuf,
    pub slug: String,
    pub body: String,
    pub theme_config: serde_json::map::Map<String, Value>,
}

impl Page {
    pub fn title(&self) -> String {
        let title = self.theme_config.get("title");
        if let Some(&Value::String(ref title)) = title {
            return title.to_owned();
        }

        if let Some(value) = title {
            if let Ok(title) = serde_json::to_string(&value) {
                return title;
            }
        }

        "Untitled".to_owned()
    }
}
