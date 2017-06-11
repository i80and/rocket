use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use parse::{Parser, Node, NodeValue};
use comrak;
use log;
use directives;

pub struct Evaluator {
    directives: HashMap<String, Box<directives::DirectiveHandler>>,
    pub parser: RefCell<Parser>,
}

impl Evaluator {
    pub fn new() -> Evaluator {
        Evaluator {
            directives: HashMap::new(),
            parser: RefCell::new(Parser::new()),
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

                    if let Some(handler) = self.directives.get(directive_name.as_ref()) {
                        return match handler.handle(self, &children[1..]) {
                                   Ok(s) => s,
                                   Err(_) => {
                                       self.error(&children[1], &format!("Error in directive {}", directive_name));
                                       return "".to_owned();
                                   }
                               };
                    }

                    println!("Unknown directive {:?}", directive_name);
                    "".to_owned()
                } else {
                    println!("Empty node");
                    "".to_owned()
                }
            }
        }
    }

    pub fn render_markdown(&self, markdown: &str) -> String {
        let mut options = comrak::ComrakOptions::default();
        options.github_pre_lang = true;
        options.ext_strikethrough = true;
        options.ext_table = true;

        comrak::markdown_to_html(markdown, &options)
    }

    pub fn log(&self, node: &Node, message: &str, level: log::LogLevel) {
        let parser = self.parser.borrow();
        let file_path = parser.get_node_source_path(node);
        log!(level, "{}\n  --> {}:?:?", message, file_path.unwrap_or_else(|| Path::new("")).to_string_lossy());
    }

    #[allow(dead_code)]
    pub fn warn(&self, node: &Node, message: &str) {
        self.log(node, message, log::LogLevel::Warn);
    }

    pub fn error(&self, node: &Node, message: &str) {
        self.log(node, message, log::LogLevel::Error);
    }
}
