use std::borrow::Cow;
use std::rc::Rc;
use std::collections::HashMap;
use std::path::Path;
use log;
use serde_json;
use directives;
use highlighter::{self, SyntaxHighlighter};
use markdown;
use page::Slug;
use parse::{Parser, Node, NodeValue};
use toctree::TocTree;

pub struct Evaluator {
    directives: HashMap<String, Rc<directives::DirectiveHandler>>,
    current_slug: Option<Slug>,

    pub parser: Parser,
    pub markdown: markdown::MarkdownRenderer,
    pub highlighter: SyntaxHighlighter,

    pub variable_stack: Vec<(String, String)>,
    pub ctx: HashMap<String, NodeValue>,
    pub theme_config: serde_json::map::Map<String, serde_json::Value>,
    pub toctree: TocTree,
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
            parser: Parser::new(),
            markdown: markdown::MarkdownRenderer::new(),
            highlighter: SyntaxHighlighter::new(syntax_theme),
            variable_stack: vec![],
            ctx: HashMap::new(),
            theme_config: serde_json::map::Map::new(),
            toctree: TocTree::new(Slug::new("index".to_owned()), true),
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
