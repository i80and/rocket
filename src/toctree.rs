use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

// This is all ported over from an earlier Python module I wrote.
// Parts... might be a little funky.

pub struct TocTree {
    titles: HashMap<String, String>,
    parent: HashMap<String, Option<String>>,
    children: HashMap<String, Vec<String>>,
    roots: Vec<String>,
    orphans: HashSet<String>,

    linear_order: HashMap<String, u32>,
    next_slug: HashMap<String, String>,
    prev_slug: HashMap<String, String>,

    // Use memoization to avoid needlessly recomputing unchanged
    // parts of the TOC tree. Maps sets of slugs "a b c" to a string
    cache: RefCell<HashMap<String, String>>,
}

impl TocTree {
    pub fn new() -> Self {
        TocTree {
            titles: HashMap::new(),
            parent: HashMap::new(),
            children: HashMap::new(),
            roots: vec![],
            orphans: HashSet::new(),

            linear_order: HashMap::new(),
            next_slug: HashMap::new(),
            prev_slug: HashMap::new(),

            cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn add(&mut self, slug: String, title: String, parent: Option<String>, next_slug: String) {
        self.titles.insert(slug.to_owned(), title);
        self.parent.insert(slug.to_owned(), parent.to_owned());

        self.next_slug.insert(slug.to_owned(), next_slug.to_owned());
        self.prev_slug.insert(next_slug, slug.to_owned());

        if parent.is_none() {
            self.orphans.insert(slug);
        }
    }

    /// Return True if slug is a child/grand-child/... of ancestor.
    fn is_child_of(&self, slug: &str, ancestor: &str) -> bool {
        if slug == ancestor {
            return true;
        }

        match self.parent.get(slug) {
            Some(&Some(ref slug)) => self.is_child_of(slug, ancestor),
            _ => false,
        }
    }

    /// Create an inverted tree for looking up children.
    pub fn finish(&mut self) -> Result<(), ()> {
        for (slug, parent) in &self.parent {
            let parent = match *parent {
                Some(ref p) => p,
                None => continue,
            };

            let mut have_children = false;
            if let Some(children) = self.children.get_mut(parent) {
                children.push(slug.to_owned());
                have_children = true;
            }

            if !have_children {
                self.children.insert(parent.to_owned(), vec![slug.to_owned()]);
            }
        }

        for orphan in &self.orphans {
            if let Some(child) = self.children.get(orphan) {
                if !child.is_empty() {
                    self.roots.push(orphan.to_owned());
                }
            }
        }

        // Create our linear order list
        let mut cur_page: Option<&str> = None;
        for key in self.next_slug.keys() {
            if !self.orphans.contains(key) {
                cur_page = Some(&key);
            }
        }

        let mut cur_page = match cur_page {
            Some(p) => p,
            None => {
                // No non-orphan pages
                return Err(());
            }
        };

        while self.prev_slug.contains_key(cur_page) {
            cur_page = &self.prev_slug[cur_page];
        }

        let mut index = 0;
        while self.next_slug.contains_key(cur_page) {
            self.linear_order.insert(cur_page.to_owned(), index);
            index += 1;
            cur_page = &self.next_slug[cur_page];
        }

        if self.roots.is_empty() {
            // Failed to find root
            return Err(());
        }

        Ok(())
    }

    fn subtree_html(&self, cur_slug: &str, level: u8, mut slugs: Vec<String>) -> Result<Vec<Cow<'static, str>>, ()> {
        if self.roots.is_empty() {
            // No roots in toctree
            return Err(());
        }

        // Properly order our slugs
        slugs.sort_by_key(|slug| self.linear_order[slug]);

        let mut cur_slug_depth = cur_slug.bytes().filter(|b| *b == '/' as u8).count();
        if !cur_slug.ends_with("index") {
            cur_slug_depth += 1;
        }
        let slug_prefix = "../".repeat(cur_slug_depth);
        let mut tokens = vec![];
        tokens.push(Cow::Borrowed(r#"<ul class="current">r"#));
        for slug in &slugs {
            let current = if self.is_child_of(cur_slug, &slug) {
                Cow::Borrowed("current")
            } else {
                Cow::Borrowed("")
            };

            let li_element = format!(r#"<li class="toctree-l{} {}"><a class="reference internal {}" href="{}">{}</a>"#,
                                     level, current, current, slug_prefix.to_owned() + &slug, self.titles[slug]);
            tokens.push(Cow::Owned(li_element));

            let children = match self.children.get(slug) {
                Some(children) if !children.is_empty() => Some(children),
                _ => None,
            };

            if let Some(children) = children {
                tokens.push(Cow::Borrowed("<ul>"));
                let key = slugs.join(" ");
                if !current.is_empty() || !self.cache.borrow().contains_key(&key) {
                    let rendered_children = self.subtree_html(cur_slug, level+1, children.to_owned())?;
                    if current.is_empty() {
                        self.cache.borrow_mut().insert(key.to_owned(), rendered_children.concat());
                    }
                    tokens.extend(rendered_children);
                } else {
                    tokens.push(Cow::Owned(self.cache.borrow()[&key].to_owned()));
                }

                tokens.push(Cow::Borrowed("</ul>"));
            } else {
                tokens.push(Cow::Borrowed("</li>"));
            }
        }

        tokens.push(Cow::Borrowed("</ul>"));
        Ok(tokens)
    }

    pub fn generate_html(&self, slug: &str) -> Result<Vec<Cow<'static, str>>, ()> {
        self.subtree_html(slug, 1, self.roots.clone())
    }
}
