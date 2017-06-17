use std::borrow::Cow;
use std::collections::HashMap;

#[derive(Debug)]
struct TocTreeElement {
    slug: String,
    title: Option<String>,
}

pub struct TocTree {
    /// Maps parent -> children
    children: HashMap<String, Vec<TocTreeElement>>,

    /// Maps child -> parents
    inverse_children: HashMap<String, Vec<String>>,

    titles: HashMap<String, String>,
}

impl TocTree {
    pub fn new() -> Self {
        TocTree {
            children: HashMap::new(),
            inverse_children: HashMap::new(),
            titles: HashMap::new(),
        }
    }

    pub fn add(&mut self, parent_slug: String, child: String, title: Option<String>) {
        let new_element = TocTreeElement {
            slug: child.to_owned(),
            title: title,
        };

        self.inverse_children
            .entry(child)
            .or_insert_with(|| vec![parent_slug.to_owned()]);
        self.children
            .entry(parent_slug)
            .or_insert_with(|| vec![new_element]);
    }

    pub fn finish(&mut self, titles: HashMap<String, String>) {
        self.titles = titles;
    }

    pub fn generate_html(&self, slug: &str) -> Result<Vec<Cow<'static, str>>, String> {
        let root = match self.children.get(slug) {
            Some(root) => root,
            None => {
                return {
                           Ok(vec![Cow::Borrowed("")])
                       }
            }
        };

        let mut result = vec![];

        let slug_depth = slug.matches('/').count();
        let slug_depth = if !slug.ends_with("index") {
            slug_depth + 1
        } else {
            slug_depth
        };

        let slug_prefix = "../".repeat(slug_depth);
        result.push(Cow::Borrowed("<ul>"));

        for child in root {
            if self.is_child_of(slug, &child.slug) {
                result.push(Cow::Borrowed(r#"<li class="current">"#));
            } else {
                result.push(Cow::Borrowed("<li>"));
            }

            let title = match child.title.as_ref() {
                Some(t) => t,
                None => {
                    self.titles
                        .get(&child.slug)
                        .ok_or_else(|| format!("Failed to find toctree entry '{}'", &child.slug))?
                }
            };

            result.push(Cow::Owned(format!(r#"<a href="{}{}">{}</a>"#,
                                           slug_prefix,
                                           child.slug,
                                           title)));
            result.extend(self.generate_html(&child.slug)?);
            result.push(Cow::Borrowed("</li>"));
        }
        result.push(Cow::Borrowed("</ul>"));
        Ok(result)
    }

    /// Return True if slug is a child/grand-child/... of ancestor.
    fn is_child_of(&self, slug: &str, ancestor: &str) -> bool {
        if slug == ancestor {
            return true;
        }

        match self.inverse_children.get(slug) {
            Some(parents) => {
                parents
                    .iter()
                    .any(|parent| self.is_child_of(parent, ancestor))
            }
            _ => false,
        }
    }
}
