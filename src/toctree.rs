use std::borrow::Cow;
use std::collections::HashMap;
use page::Slug;

#[derive(Debug)]
struct TocTreeElement {
    slug: Slug,
    title: Option<String>,
}

pub struct TocTree {
    root: Slug,

    /// Maps parent -> children
    children: HashMap<Slug, Vec<TocTreeElement>>,

    /// Maps child -> parents
    inverse_children: HashMap<Slug, Vec<Slug>>,

    titles: HashMap<Slug, String>,
    pretty_url: bool,
}

impl TocTree {
    pub fn new(root: Slug, pretty_url: bool) -> Self {
        TocTree {
            root: root,
            children: HashMap::new(),
            inverse_children: HashMap::new(),
            titles: HashMap::new(),
            pretty_url: pretty_url,
        }
    }

    pub fn new_empty() -> Self {
        Self::new(Slug::new("".to_owned()), false)
    }

    pub fn add(&mut self, parent_slug: &Slug, child: Slug, title: Option<String>) {
        let new_element = TocTreeElement {
            slug: child.to_owned(),
            title: title,
        };

        self.inverse_children
            .entry(child)
            .or_insert_with(|| vec![])
            .push(parent_slug.to_owned());
        self.children
            .entry(parent_slug.to_owned())
            .or_insert_with(|| vec![])
            .push(new_element);
    }

    pub fn finish(&mut self, titles: HashMap<Slug, String>) {
        self.titles = titles;
    }

    pub fn generate_html(
        &self,
        root: &Slug,
        current_slug: &Slug,
        is_root: bool,
    ) -> Result<Vec<Cow<'static, str>>, String> {
        let children = match self.children.get(root) {
            Some(children) => children,
            None => {
                return Ok(vec![Cow::Borrowed("")]);
            }
        };

        let mut result = vec![];
        result.push(Cow::Borrowed("<ul>"));

        if is_root {
            result.push(Cow::Borrowed(r#"<li class="current">"#));
            let title = self.titles
                .get(&self.root)
                .ok_or_else(|| format!("Failed to find toctree root '{}'", &self.root))?;
            result.push(Cow::Owned(format!(
                r#"<a href="{}">{}</a>"#,
                current_slug.path_to(&Slug::new("".to_owned()), self.pretty_url),
                title
            )));
            result.push(Cow::Borrowed("</li>"));
        }

        for child in children {
            if self.is_ancestor_of(&child.slug, current_slug) {
                result.push(Cow::Borrowed(r#"<li class="current">"#));
            } else {
                result.push(Cow::Borrowed("<li>"));
            }

            let title = match child.title.as_ref() {
                Some(t) => t,
                None => self.titles
                    .get(&child.slug)
                    .ok_or_else(|| format!("Failed to find toctree entry '{}'", &child.slug))?,
            };

            result.push(Cow::Owned(format!(
                r#"<a href="{}">{}</a>"#,
                current_slug.path_to(&child.slug, self.pretty_url),
                title
            )));
            result.extend(self.generate_html(&child.slug, current_slug, false)?);
            result.push(Cow::Borrowed("</li>"));
        }
        result.push(Cow::Borrowed("</ul>"));

        if is_root {
            result.push(Cow::Borrowed("</ul>"));
        }

        Ok(result)
    }

    /// Return True if ancestor is a parent/grand-parent/... of child.
    fn is_ancestor_of(&self, ancestor: &Slug, child: &Slug) -> bool {
        if ancestor == child {
            return true;
        }

        match self.inverse_children.get(child) {
            Some(parents) => parents
                .iter()
                .any(|parent| self.is_ancestor_of(ancestor, parent)),
            _ => false,
        }
    }
}
