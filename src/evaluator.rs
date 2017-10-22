use std::borrow::Cow;
use std::rc::Rc;
use std::collections::HashMap;
use std::path::Path;
use log;
use serde_json;
use rand;
use rand::Rng;
use regex::{Captures, Regex};
use directives;
use highlighter::{self, SyntaxHighlighter};
use markdown;
use page::{Slug, Page};
use parse::{Parser, Node, NodeValue};
use toctree::TocTree;

pub enum PlaceholderAction { Path, Title, }

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

pub struct Evaluator {
    directives: HashMap<String, Rc<directives::DirectiveHandler>>,
    current_slug: Option<Slug>,

    pub parser: Parser,
    pub markdown: markdown::MarkdownRenderer,
    pub highlighter: SyntaxHighlighter,

    pub variable_stack: Vec<(String, String)>,
    pub ctx: HashMap<String, NodeValue>,
    pub refdefs: HashMap<String, RefDef>,
    pub theme_config: serde_json::map::Map<String, serde_json::Value>,
    pub toctree: TocTree,

    placeholder_pattern: Regex,
    placeholder_prefix: String,
    pub pending_links: Vec<(PlaceholderAction, String)>,
}

impl Evaluator {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::new_with_options(highlighter::DEFAULT_SYNTAX_THEME)
    }

    pub fn new_with_options(syntax_theme: &str) -> Self {
        let hex_chars = "0123456789abcdef".as_bytes();
        let mut rnd_buf = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut rnd_buf);
        let mut placeholder_prefix = String::with_capacity(32);
        for c in rnd_buf.iter() {
            placeholder_prefix.push(hex_chars[(c >> 4) as usize] as char);
            placeholder_prefix.push(hex_chars[(c & 15) as usize] as char);
        }

        let pattern_text = format!(r"%{}-(\d+)%", &placeholder_prefix);
        let placeholder_pattern = Regex::new(&pattern_text).expect("Failed to compile linker pattern");

        Evaluator {
            directives: HashMap::new(),
            current_slug: None,
            parser: Parser::new(),
            markdown: markdown::MarkdownRenderer::new(),
            highlighter: SyntaxHighlighter::new(syntax_theme),
            variable_stack: vec![],
            ctx: HashMap::new(),
            refdefs: HashMap::new(),
            theme_config: serde_json::map::Map::new(),
            toctree: TocTree::new(Slug::new("index".to_owned()), true),

            placeholder_pattern,
            placeholder_prefix,
            pending_links: vec![],
        }
    }

    pub fn register<S: Into<String>>(&mut self,
                                     name: S,
                                     handler: Rc<directives::DirectiveHandler>) {
        self.directives.insert(name.into(), handler);
    }

    pub fn evaluate(&mut self, node: &Node) -> String {
        match node.value {
            NodeValue::Owned(ref s) => s.to_owned(),
            NodeValue::Children(ref children) => {
                if let Some(first_element) = children.get(0) {
                    let directive_name = match first_element.value {
                        NodeValue::Owned(ref dname) => Cow::Borrowed(dname),
                        NodeValue::Children(_) => Cow::Owned(self.evaluate(first_element)),
                    };

                    self.lookup(node, directive_name.as_ref(), &children[1..])
                        .unwrap_or_else(|_| "".to_owned())
                } else {
                    println!("Empty node");
                    "".to_owned()
                }
            }
        }
    }

    pub fn log(&self, node: &Node, message: &str, level: log::LogLevel) {
        let file_path = self.parser.get_node_source_path(node);
        log!(level,
             "{}\n  --> {}:?:?",
             message,
             file_path.unwrap_or_else(|| Path::new("")).to_string_lossy());
    }

    #[allow(dead_code)]
    pub fn warn(&self, node: &Node, message: &str) {
        self.log(node, message, log::LogLevel::Warn);
    }

    pub fn error(&self, node: &Node, message: &str) {
        self.log(node, message, log::LogLevel::Error);
    }

    pub fn reset(&mut self) {
        self.ctx.clear();
        self.theme_config.clear();
        self.variable_stack.clear();
    }

    pub fn set_slug(&mut self, slug: Slug) {
        self.current_slug = Some(slug);
    }

    pub fn get_slug(&self) -> &Slug {
        self.current_slug
            .as_ref()
            .expect("Requested slug before set")
    }

    pub fn get_placeholder(&mut self, refid: String, action: PlaceholderAction) -> String {
        self.pending_links.push((action, refid));
        format!("%{}-{}%", self.placeholder_prefix, self.pending_links.len() - 1)
    }

    pub fn substitute(&self, page: &Page) -> Result<String, ()> {
        let result = self.placeholder_pattern.replace_all(&page.body, |captures: &Captures| {
            let ref_number = str::parse::<u64>(&captures[1]).expect("Failed to parse refid");
            let &(ref action, ref refid) = self.pending_links.get(ref_number as usize).expect("Missing ref number");
            let refdef = match self.refdefs.get(refid) {
                Some(r) => r,
                None => {
                    return format!("unknown refdef");
                },
            };

            match action {
                &PlaceholderAction::Path => page.slug.path_to(&refdef.slug, true),
                &PlaceholderAction::Title => refdef.title.to_owned(),
            }
        });

        Ok(result.into_owned())
    }

    fn lookup(&mut self, node: &Node, key: &str, args: &[Node]) -> Result<String, ()> {
        let var = self.variable_stack
            .iter()
            .rev()
            .find(|&&(ref k, _)| k == key)
            .map(|&(_, ref v)| v.to_owned());

        if let Some(value) = var {
            if !args.is_empty() {
                return Err(());
            }

            return Ok(value.to_owned());
        }

        let handler = match self.directives.get(key) {
            Some(handler) => handler.clone(),
            None => {
                self.error(node, &format!("Unknown directive {}", key));
                return Err(());
            }
        };

        return match handler.handle(self, args) {
            Ok(result) => Ok(result),
            Err(_) => {
                self.error(node, &format!("Error in directive {}", key));
                Err(())
            }
        };
    }
}
