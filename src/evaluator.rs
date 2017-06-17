use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use log;
use serde_json;
use directives;
use highlighter::{self, SyntaxHighlighter};
use markdown;
use parse::{Parser, Node, NodeValue};
use toctree::TocTree;

pub struct Evaluator {
    directives: HashMap<String, Box<directives::DirectiveHandler>>,
    current_slug: Option<String>,

    pub parser: RefCell<Parser>,
    pub markdown: markdown::MarkdownRenderer,
    pub highlighter: SyntaxHighlighter,

    pub variable_stack: RefCell<Vec<(String, String)>>,
    pub ctx: RefCell<HashMap<String, NodeValue>>,
    pub theme_config: RefCell<serde_json::map::Map<String, serde_json::Value>>,
    pub toctree: RefCell<TocTree>,
}

impl Evaluator {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::new_with_options(highlighter::DEFAULT_SYNTAX_THEME)
    }

    pub fn new_with_options(syntax_theme: &str) -> Self {
        Evaluator {
            directives: HashMap::new(),
            current_slug: None,
            parser: RefCell::new(Parser::new()),
            markdown: markdown::MarkdownRenderer::new(),
            highlighter: SyntaxHighlighter::new(syntax_theme),
            variable_stack: RefCell::new(vec![]),
            ctx: RefCell::new(HashMap::new()),
            theme_config: RefCell::new(serde_json::map::Map::new()),
            toctree: RefCell::new(TocTree::new()),
        }
    }

    pub fn register<S: Into<String>>(&mut self,
                                     name: S,
                                     handler: Box<directives::DirectiveHandler>) {
        self.directives.insert(name.into(), handler);
    }

    pub fn evaluate(&self, node: &Node) -> String {
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
        let parser = self.parser.borrow();
        let file_path = parser.get_node_source_path(node);
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

    pub fn reset(&self) {
        self.ctx.borrow_mut().clear();
        self.theme_config.borrow_mut().clear();
        self.variable_stack.borrow_mut().clear();
    }

    pub fn set_slug(&mut self, slug: &str) {
        self.current_slug = Some(slug.to_owned());
    }

    pub fn get_slug(&self) -> &str {
        self.current_slug
            .as_ref()
            .expect("Requested slug before set")
    }

    fn lookup(&self, node: &Node, key: &str, args: &[Node]) -> Result<String, ()> {
        let var = self.variable_stack
            .borrow()
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

        if let Some(handler) = self.directives.get(key) {
            return match handler.handle(self, args) {
                       Ok(result) => Ok(result),
                       Err(_) => {
                           self.error(node, &format!("Error in directive {}", key));
                           Err(())
                       }
                   };
        }

        self.error(node, &format!("Unknown directive {}", key));
        Err(())
    }
}
