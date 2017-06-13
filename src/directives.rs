use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde_json;
use parse::{Node, NodeValue};
use evaluator::Evaluator;

pub trait DirectiveHandler {
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()>;
}

pub struct Dummy;

impl Dummy {
    pub fn new() -> Dummy {
        Dummy
    }
}

impl DirectiveHandler for Dummy {
    #[allow(unused_variables)]
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()> {
        Ok("".to_owned())
    }
}

pub struct Version {
    version: Vec<String>,
}

impl Version {
    pub fn new(version: &str) -> Version {
        Version { version: version.split('.').map(|s| s.to_owned()).collect::<Vec<_>>() }
    }
}

impl DirectiveHandler for Version {
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()> {
        match args.len() {
            0 => Ok(self.version.join(".")),
            1 => {
                let arg = evaluator.evaluate(&args[0]);
                if arg.is_empty() {
                    return Ok("".to_owned());
                }

                let n_components = arg.matches('.').count() + 1;
                Ok(self.version[..n_components].join("."))
            }
            _ => Err(()),
        }
    }
}

pub struct Admonition {
    title: String,
    class: String,
}

impl Admonition {
    pub fn new(title: &str, class: &str) -> Admonition {
        Admonition {
            title: title.to_owned(),
            class: class.to_owned(),
        }
    }
}

impl DirectiveHandler for Admonition {
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()> {
        let mut title = self.title.to_owned();
        let raw_body = match args.len() {
            1 => evaluator.evaluate(&args[0]),
            2 => {
                title = evaluator.evaluate(&args[0]);
                evaluator.evaluate(&args[1])
            }
            _ => return Err(()),
        };

        let body = evaluator.markdown.render(&raw_body, &evaluator.highlighter);
        Ok(format!("<div class=\"admonition admonition-{}\"><span class=\"admonition-title admonition-title-{}\">{}</span>{}</div>\n",
                   self.class,
                   self.class,
                   title,
                   body))
    }
}

pub struct Concat;

impl Concat {
    pub fn new() -> Concat {
        Concat
    }
}

impl DirectiveHandler for Concat {
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()> {
        Ok(args.iter()
               .map(|node| evaluator.evaluate(node))
               .fold(String::new(), |r, c| r + &c))
    }
}

pub struct Markdown;

impl Markdown {
    pub fn new() -> Markdown {
        Markdown
    }
}

impl DirectiveHandler for Markdown {
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()> {
        let body = args.iter()
            .map(|node| evaluator.evaluate(node))
            .fold(String::new(), |r, c| r + &c);

        let rendered = evaluator
            .markdown
            .render(&body, &evaluator.highlighter)
            .trim()
            .to_owned();
        Ok(rendered)
    }
}

pub struct LinkTemplate {
    prefix: String,
}

impl LinkTemplate {
    pub fn new(prefix: &str) -> LinkTemplate {
        LinkTemplate { prefix: prefix.to_owned() }
    }
}

impl DirectiveHandler for LinkTemplate {
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()> {
        let mut title = None;
        let suffix = match args.len() {
            1 => evaluator.evaluate(&args[0]),
            2 => {
                title = Some(evaluator.evaluate(&args[0]));
                evaluator.evaluate(&args[1])
            }
            _ => return Err(()),
        };

        let url = self.prefix.clone() + &suffix;
        let title = match title {
            Some(t) => t,
            None => url.clone(),
        };

        Ok(format!(r#"<a href="{}">{}</a>"#, url, title))
    }
}

pub struct DefinitionList;

impl DefinitionList {
    pub fn new() -> DefinitionList {
        DefinitionList
    }
}

impl DirectiveHandler for DefinitionList {
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()> {
        let segments: Result<Vec<_>, _> = args.iter()
            .map(|node| match node.value {
                     NodeValue::Owned(_) => Err(()),
                     NodeValue::Children(ref children) => {
                         if children.len() != 2 {
                             return Err(());
                         }

                         let term = evaluator.evaluate(&children[0]);
                         let definition =
                             evaluator
                                 .markdown
                                 .render(&evaluator.evaluate(&children[1]),
                                         &evaluator.highlighter);
                         Ok(format!("<dt>{}</dt><dd>{}</dd>", term, definition))
                     }
                 })
            .collect();

        match segments {
            Ok(s) => Ok(s.concat()),
            Err(_) => Err(()),
        }
    }
}

pub struct Include;

impl Include {
    pub fn new() -> Include {
        Include
    }
}

impl DirectiveHandler for Include {
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()> {
        if args.len() != 1 {
            return Err(());
        }

        let mut path = PathBuf::from(evaluator.evaluate(&args[0]));
        if !path.is_absolute() {
            let parser = evaluator.parser.borrow();
            let prefix = parser
                .get_node_source_path(&args[0])
                .expect("Node with unknown file ID")
                .parent()
                .unwrap_or_else(|| Path::new(""));
            path = prefix.join(path.to_owned());
        }

        let node = match evaluator.parser.borrow_mut().parse(path.as_ref()) {
            Ok(n) => n,
            Err(_) => return Err(()),
        };

        Ok(evaluator.evaluate(&node))
    }
}

pub struct Let;

impl Let {
    pub fn new() -> Let {
        Let
    }
}

impl DirectiveHandler for Let {
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()> {
        if args.len() < 1 {
            return Err(());
        }

        let mut replacements = HashMap::new();
        let kvs = &args[0];
        match kvs.value {
            NodeValue::Owned(_) => {
                return Err(());
            }
            NodeValue::Children(ref children) => {
                if children.len() % 2 != 0 {
                    return Err(());
                }

                for pair in children.chunks(2) {
                    let evaluated_key = evaluator.evaluate(&pair[0]);
                    let evaluated_value = evaluator.evaluate(&pair[1]);

                    replacements.insert(evaluated_key, evaluated_value);
                }
            }
        }

        let mut result = String::with_capacity(128);

        for node in &args[1..] {
            let new_node = node.map(&|candidate| match candidate.value {
                                        NodeValue::Owned(_) => None,
                                        NodeValue::Children(ref children) => {
                                            if children.is_empty() {
                                                return None;
                                            }

                                            if let NodeValue::Owned(ref key) = children[0].value {
                                                if let Some(new_value) = replacements.get(key) {
                        return Some(Node {
                                        value: NodeValue::Owned(new_value.to_owned()),
                                        file_id: candidate.file_id,
                                    });
                    }
                                            }

                                            None
                                        }
                                    });

            result += &evaluator.evaluate(&new_node);
        }

        Ok(result)
    }
}

pub struct Define;

impl Define {
    pub fn new() -> Define {
        Define
    }
}

impl DirectiveHandler for Define {
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()> {
        if args.len() != 2 {
            return Err(());
        }

        let key = evaluator.evaluate(&args[0]);

        evaluator
            .ctx
            .borrow_mut()
            .insert(key.to_owned(), args[1].value.clone());
        Ok("".to_owned())
    }
}

pub struct Get;

impl Get {
    pub fn new() -> Get {
        Get
    }
}

impl DirectiveHandler for Get {
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()> {
        if args.len() != 1 {
            return Err(());
        }

        let key = evaluator.evaluate(&args[0]);
        match evaluator.ctx.borrow().get(&key) {
            Some(value) => {
                let node = Node {
                    value: value.clone(),
                    file_id: args[0].file_id,
                };
                Ok(evaluator.evaluate(&node))
            }
            None => Err(()),
        }
    }
}

pub struct ThemeConfig;

impl ThemeConfig {
    pub fn new() -> ThemeConfig {
        ThemeConfig
    }
}

impl DirectiveHandler for ThemeConfig {
    fn handle(&self, evaluator: &Evaluator, args: &[Node]) -> Result<String, ()> {
        if args.len() % 2 != 0 {
            return Err(());
        }

        for pair in args.chunks(2) {
            let key = evaluator.evaluate(&pair[0]);
            let value = evaluator.evaluate(&pair[1]);

            evaluator
                .theme_config
                .borrow_mut()
                .insert(key, serde_json::Value::String(value));
        }

        Ok("".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dummy() {
        let evaluator = Evaluator::new();
        let handler = Dummy::new();

        assert_eq!(handler.handle(&evaluator, &[]), Ok("".to_owned()));
        assert_eq!(handler.handle(&evaluator, &[Node::new_string("")]),
                   Ok("".to_owned()));
        assert_eq!(handler.handle(&evaluator,
                                  &[Node::new_children(vec![Node::new_string("")])]),
                   Ok("".to_owned()));
    }

    #[test]
    fn test_version() {
        let mut evaluator = Evaluator::new();
        evaluator.register("concat", Box::new(Concat::new()));
        let handler = Version::new("3.4.0");

        assert_eq!(handler.handle(&evaluator, &[]), Ok("3.4.0".to_owned()));
        assert_eq!(handler.handle(&evaluator, &[Node::new_string("")]),
                   Ok("".to_owned()));
        assert_eq!(handler.handle(&evaluator, &[Node::new_string("x")]),
                   Ok("3".to_owned()));
        assert_eq!(handler.handle(&evaluator, &[Node::new_string("x.y")]),
                   Ok("3.4".to_owned()));

        assert_eq!(handler.handle(&evaluator,
                                  &[Node::new_children(vec![Node::new_string("concat"),
                                                            Node::new_string("3."),
                                                            Node::new_string("4")])]),
                   Ok("3.4".to_owned()));
    }

    #[test]
    fn test_admonition() {
        let evaluator = Evaluator::new();
        let handler = Admonition::new("note", "Note");

        assert!(handler.handle(&evaluator, &[]).is_err());
        assert!(handler
                    .handle(&evaluator, &[Node::new_string("foo")])
                    .is_ok());
    }

    #[test]
    fn test_concat() {
        let mut evaluator = Evaluator::new();
        evaluator.register("version", Box::new(Version::new("3.4")));
        let handler = Concat::new();

        assert_eq!(handler.handle(&evaluator, &[]), Ok("".to_owned()));
        assert_eq!(handler.handle(&evaluator, &[Node::new_string("foo")]),
                   Ok("foo".to_owned()));
        assert_eq!(handler.handle(&evaluator,
                                  &[Node::new_string("foo"),
                                    Node::new_string("bar"),
                                    Node::new_string("baz")]),
                   Ok("foobarbaz".to_owned()));

        assert_eq!(handler.handle(&evaluator,
                                  &[Node::new_children(vec![Node::new_string("version")]),
                                    Node::new_string("-test")]),
                   Ok("3.4-test".to_owned()));
    }

    #[test]
    fn test_markdown() {
        let evaluator = Evaluator::new();
        let handler = Markdown::new();

        assert_eq!(handler.handle(&evaluator, &[]), Ok("".to_owned()));
        assert_eq!(handler.handle(&evaluator, &[Node::new_string("Some *markdown* text")]),
                   Ok("<p>Some <em>markdown</em> text</p>".to_owned()));
    }

    #[test]
    fn test_link_template() {
        let evaluator = Evaluator::new();
        let handler = LinkTemplate::new("https://foxquill.com");

        assert!(handler.handle(&evaluator, &[]).is_err());
        assert_eq!(handler.handle(&evaluator, &[Node::new_string("/simd-rectangle-intersection/")]),
                   Ok(r#"<a href="https://foxquill.com/simd-rectangle-intersection/">https://foxquill.com/simd-rectangle-intersection/</a>"#.to_owned()));
        assert_eq!(handler.handle(&evaluator, &[Node::new_string("SIMD.js Rectangle Intersection"), Node::new_string("/simd-rectangle-intersection/")]),
                   Ok(r#"<a href="https://foxquill.com/simd-rectangle-intersection/">SIMD.js Rectangle Intersection</a>"#.to_owned()));
    }

    #[test]
    fn test_let() {
        let mut evaluator = Evaluator::new();
        let handler = Let::new();

        evaluator.register("concat", Box::new(Concat::new()));

        assert!(handler.handle(&evaluator, &[]).is_err());
        let result =
            handler.handle(&evaluator,
                           &[Node::new_children(vec![Node::new_string("foo"),
                                              Node::new_children(vec![Node::new_string("concat",),
                                                                      Node::new_string("1"),
                                                                      Node::new_string("2")]),
                                              Node::new_string("bar"),
                                              Node::new_string("3")]),
                             Node::new_children(vec![Node::new_string("foo")]),
                             Node::new_children(vec![Node::new_string("bar")])]);

        assert_eq!(result, Ok("123".to_owned()));
    }

    #[test]
    fn test_define() {
        let mut evaluator = Evaluator::new();
        evaluator.register("concat", Box::new(Concat::new()));
        let handler = Define::new();

        assert!(handler.handle(&evaluator, &[]).is_err());
        assert_eq!(handler.handle(&evaluator,
                                  &[Node::new_string("x"),
                                    Node::new_children(vec![Node::new_string("concat"),
                                                            Node::new_string("foo"),
                                                            Node::new_string("bar")])]),
                   Ok("".to_owned()));

        assert_eq!(evaluator.evaluate(&Node {
                                          value: evaluator.ctx.borrow().get("x").unwrap().clone(),
                                          file_id: 0,
                                      }),
                   "foobar".to_owned());
    }

    #[test]
    fn test_get() {
        let mut evaluator = Evaluator::new();
        evaluator.register("concat", Box::new(Concat::new()));
        let handler = Get::new();

        evaluator
            .ctx
            .borrow_mut()
            .insert("foo".to_owned(),
                    NodeValue::Children(vec![Node::new_string("concat"),
                                             Node::new_string("foo"),
                                             Node::new_string("bar")]));
        assert!(handler.handle(&evaluator, &[]).is_err());
        assert_eq!(handler.handle(&evaluator, &[Node::new_string("foo")]),
                   Ok("foobar".to_owned()));
    }

    #[test]
    fn test_theme_config() {
        let evaluator = Evaluator::new();
        let handler = ThemeConfig::new();

        assert_eq!(handler.handle(&evaluator, &[]), Ok("".to_owned()));
        assert_eq!(handler.handle(&evaluator,
                                  &[Node::new_string("foo"), Node::new_string("bar")]),
                   Ok("".to_owned()));
        assert_eq!(evaluator.theme_config.borrow().get("foo"),
                   Some(&serde_json::Value::String("bar".to_owned())));
    }
}
