use std::path::PathBuf;
use serde_json;

pub struct Page {
    pub source_path: PathBuf,
    pub slug: String,
    pub body: String,
    pub theme_config: serde_json::map::Map<String, serde_json::Value>,
}
