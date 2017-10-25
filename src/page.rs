use std::convert;
use std::fmt;
use std::path::{Path, PathBuf};
use serde_json::{self, Value};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Slug {
    slug: String,
}

impl Slug {
    pub fn new(slug: String) -> Self {
        Slug { slug: slug }
    }

    pub fn create_output_path(&self, prefix: &Path, pretty_url: bool) -> PathBuf {
        let mut output_path = prefix.join(&self.slug);
        if pretty_url && self.slug != "index" {
            output_path.push("index");
        }
        output_path.set_extension("html");
        output_path
    }

    pub fn depth(&self, pretty_url: bool) -> usize {
        let modifier = if pretty_url && self.slug != "index" {
            1
        } else {
            0
        };

        self.slug.matches('/').count() + modifier
    }

    pub fn path_to(&self, dest: &str, pretty_url: bool) -> String {
        let slug_prefix = "../".repeat(self.depth(pretty_url));
        format!("{}{}", slug_prefix, dest)
    }
}

impl convert::AsRef<str> for Slug {
    fn as_ref(&self) -> &str {
        &self.slug
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.slug)
    }
}

pub struct Page {
    pub source_path: PathBuf,
    pub slug: Slug,
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
