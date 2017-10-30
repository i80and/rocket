use std::borrow::Cow;
use std::collections::HashMap;
use std::marker::Sync;
use std::path::Path;
use std::sync::{Arc, RwLock};
use log;
use serde_json;
use rand;
use rand::Rng;
use regex::{Captures, Regex};
use directives;
use highlighter::{self, SyntaxHighlighter};
use page::{Page, Slug};
use parse::{Node, NodeValue, Parser};
use toctree::TocTree;

pub enum PlaceholderAction {
    Path,
    Title,
}

#[derive(Debug)]
pub struct RefDef {
    pub title: String,
    pub slug: Slug,
}

impl RefDef {
    pub fn new(title: &str, slug: &Slug) -> Self {
        RefDef {
            title: title.to_owned(),
            slug: slug.to_owned(),
        }
    }
}

pub enum StoredValue {
    Directive(Box<directives::DirectiveHandler + Sync + Send>),
    Node(Node),
}

pub struct Evaluator {
    prelude_ctx: HashMap<String, Arc<StoredValue>>,
    pub refdefs: RwLock<HashMap<String, RefDef>>,
    pub toctree: RwLock<TocTree>,

    placeholder_pattern: Regex,
    placeholder_prefix: String,
    pub pending_links: RwLock<Vec<(PlaceholderAction, String)>>,
}

impl Evaluator {
    pub fn new() -> Self {
        let hex_chars = b"0123456789abcdef";
        let mut rnd_buf = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut rnd_buf);
        let mut placeholder_prefix = String::with_capacity(32);
        for c in &rnd_buf {
            placeholder_prefix.push(hex_chars[(c >> 4) as usize] as char);
            placeholder_prefix.push(hex_chars[(c & 15) as usize] as char);
        }

        let pattern_text = format!(r"%{}-(\d+)%", &placeholder_prefix);
        let placeholder_pattern =
            Regex::new(&pattern_text).expect("Failed to compile linker pattern");

        Evaluator {
            prelude_ctx: HashMap::new(),
            refdefs: RwLock::new(HashMap::new()),
            toctree: RwLock::new(TocTree::new(Slug::new("index".to_owned()), true)),

            placeholder_pattern,
            placeholder_prefix,
            pending_links: RwLock::new(vec![]),
        }
    }

    pub fn register_prelude<S: Into<String>>(
        &mut self,
        name: S,
        handler: Box<directives::DirectiveHandler + Sync + Send>,
    ) {
        self.prelude_ctx
            .insert(name.into(), Arc::new(StoredValue::Directive(handler)));
    }

    pub fn substitute(&self, page: &Page) -> Result<String, ()> {
        let result = self.placeholder_pattern
            .replace_all(&page.body, |captures: &Captures| {
                let ref_number = str::parse::<u64>(&captures[1]).expect("Failed to parse refid");
                let r1 = self.pending_links.read().unwrap();
                let r2 = self.refdefs.read().unwrap();
                let &(ref action, ref refid) =
                    r1.get(ref_number as usize).expect("Missing ref number");
                let refdef = match r2.get(refid) {
                    Some(r) => r,
                    None => {
                        error!(
                            "Unknown reference '{}' used in page {}",
                            refid,
                            page.source_path.to_string_lossy()
                        );
                        return "".to_owned();
                    }
                };

                match *action {
                    PlaceholderAction::Path => page.slug.path_to(refdef.slug.as_ref(), true),
                    PlaceholderAction::Title => refdef.title.to_owned(),
                }
            });

        Ok(result.into_owned())
    }
}

pub struct Worker<'a> {
    pub highlighter: SyntaxHighlighter,

    current_slug: Option<Slug>,
    current_level: i8,
    pub parser: Parser,

    evaluator: &'a Evaluator,
    pub ctx: HashMap<String, Arc<StoredValue>>,
    pub theme_config: serde_json::map::Map<String, serde_json::Value>,
}

impl<'a> Worker<'a> {
    #[allow(dead_code)]
    pub fn new(evaluator: &'a Evaluator) -> Self {
        Self::new_with_options(evaluator, highlighter::DEFAULT_SYNTAX_THEME)
    }

    pub fn new_with_options(evaluator: &'a Evaluator, syntax_theme: &str) -> Self {
        Worker {
            highlighter: SyntaxHighlighter::new(syntax_theme),
            current_slug: None,
            current_level: 0,
            parser: Parser::new(),
            evaluator: evaluator,
            ctx: HashMap::new(),
            theme_config: serde_json::map::Map::new(),
        }
    }

    pub fn evaluate(&mut self, node: &Node) -> String {
        match node.value {
            NodeValue::Owned(ref s) => s.to_owned(),
            NodeValue::Children(ref children) => if let Some(first_element) = children.get(0) {
                let directive_name = match first_element.value {
                    NodeValue::Owned(ref dname) => Cow::Borrowed(dname),
                    NodeValue::Children(_) => Cow::Owned(self.evaluate(first_element)),
                };

                match self.lookup(node, directive_name.as_ref(), &children[1..]) {
                    Ok(s) => s,
                    Err(_) => {
                        self.error(node, "Error evaluating node");
                        String::new()
                    }
                }
            } else {
                "".to_owned()
            },
        }
    }

    pub fn lookup(&mut self, node: &Node, key: &str, args: &[Node]) -> Result<String, ()> {
        let stored = match self.ctx
            .get(key)
            .or_else(|| self.evaluator.prelude_ctx.get(key))
        {
            Some(val) => Arc::clone(val),
            None => {
                self.error(node, &format!("Unknown name: '{}'", key));
                return Err(());
            }
        };

        match *stored {
            StoredValue::Node(ref stored_node) => Ok(self.evaluate(stored_node)),
            StoredValue::Directive(ref handler) => handler.handle(self, args),
        }
    }

    pub fn set_slug(&mut self, slug: Slug) {
        self.current_slug = Some(slug);
        self.current_level = 0;
        self.ctx.clear();
        self.theme_config.clear();
    }

    pub fn get_slug(&self) -> &Slug {
        self.current_slug
            .as_ref()
            .expect("Requested slug before set")
    }

    pub fn add_asset(&self, path: &str) -> Result<String, ()> {
        let output_slug = Slug::new(format!("_static/{}", path));
        let slug = self.current_slug
            .as_ref()
            .expect("current_slug not yet initialized");
        Ok(slug.path_to(output_slug.as_ref(), true))
    }

    pub fn register<S: Into<String>>(
        &mut self,
        name: S,
        handler: Box<directives::DirectiveHandler + Sync + Send>,
    ) {
        self.ctx
            .insert(name.into(), Arc::new(StoredValue::Directive(handler)));
    }

    pub fn get_placeholder(&mut self, refid: String, action: PlaceholderAction) -> String {
        let mut txn = self.evaluator.pending_links.write().unwrap();
        txn.push((action, refid));
        format!("%{}-{}%", self.evaluator.placeholder_prefix, txn.len() - 1)
    }

    pub fn insert_refdef(&self, refid: String, refdef: RefDef) {
        self.evaluator
            .refdefs
            .write()
            .unwrap()
            .insert(refid, refdef);
    }

    pub fn add_to_toctree(&self, slug: Slug, title: Option<String>) {
        let current_slug = self.current_slug.as_ref().unwrap();
        self.evaluator
            .toctree
            .write()
            .unwrap()
            .add(current_slug, slug, title);
    }

    pub fn handle_heading(&mut self, level: i8) -> Result<String, ()> {
        let prefix = if level == self.current_level + 1 {
            "<section>".to_owned()
        } else if level == self.current_level {
            "".to_owned()
        } else if level < self.current_level {
            "</section>".repeat((self.current_level - level) as usize)
        } else {
            return Err(());
        };

        self.current_level = level;
        Ok(prefix)
    }

    pub fn close_sections(&self) -> String {
        "</section>".repeat(self.current_level as usize)
    }

    pub fn log(&self, node: &Node, message: &str, level: log::LogLevel) {
        let file_path = self.parser.get_node_source_path(node);
        log!(
            level,
            "{}\n  --> {}:{}:?",
            message,
            file_path.unwrap_or_else(|| Path::new("")).to_string_lossy(),
            if node.lineno >= 0 {
                node.lineno.to_string()
            } else {
                "?".to_owned()
            }
        );
    }

    #[allow(dead_code)]
    pub fn warn(&self, node: &Node, message: &str) {
        self.log(node, message, log::LogLevel::Warn);
    }

    pub fn error(&self, node: &Node, message: &str) {
        self.log(node, message, log::LogLevel::Error);
    }
}
