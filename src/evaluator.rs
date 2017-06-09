use std::borrow::Cow;
use std::collections::HashMap;
use comrak;
use parse::Node;
use directives;

pub struct Evaluator {
    directives: HashMap<String, Box<directives::DirectiveHandler>>,
}

impl Evaluator {
    pub fn new() -> Evaluator {
        Evaluator { directives: HashMap::new() }
    }

    pub fn register<S: Into<String>>(&mut self,
                                     name: S,
                                     handler: Box<directives::DirectiveHandler>) {
        self.directives.insert(name.into(), handler);
    }

    pub fn evaluate(&self, node: &Node) -> String {
        match node {
            &Node::Owned(ref s) => {
                return s.to_owned();
            }
            &Node::Children(ref children) => {
                if let Some(first_element) = children.get(0) {
                    let directive_name = match first_element {
                        &Node::Owned(ref dname) => Cow::Borrowed(dname),
                        &Node::Children(_) => Cow::Owned(self.evaluate(first_element)),
                    };

                    if let Some(handler) = self.directives.get(directive_name.as_ref()) {
                        return match handler.handle(self, &children[1..]) {
                                   Ok(s) => s,
                                   Err(_) => {
                                       println!("Error in directive {:?}", directive_name);
                                       return "".to_owned();
                                   }
                               };
                    }

                    println!("Unknown directive {:?}", directive_name);
                    return "".to_owned();
                } else {
                    println!("Empty node");
                    return "".to_owned();
                }
            }
        }
    }

    pub fn render_markdown(&self, markdown: &str) -> String {
        let mut options = comrak::ComrakOptions::default();
        options.github_pre_lang = true;
        options.ext_strikethrough = true;

        return comrak::markdown_to_html(&markdown, &options);
    }
}
