use std::{cmp, iter, mem, slice, str};
use std::rc::Rc;
use std::path::{Path, PathBuf};
use std::collections::hash_map::Entry;
use regex::{Captures, Regex};
use serde_json;
use parse::{Node, NodeValue};
use page::Slug;
use evaluator::{Evaluator, PlaceholderAction, RefDef, StoredValue};

fn consume_string(iter: &mut slice::Iter<Node>, evaluator: &mut Evaluator) -> Option<String> {
    match iter.next() {
        Some(n) => match n.value {
            NodeValue::Owned(ref s) => Some(s.to_owned()),
            NodeValue::Children(_) => Some(evaluator.evaluate(n)),
        },
        None => None,
    }
}

pub trait DirectiveHandler {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()>;
}

pub struct Dummy;

impl DirectiveHandler for Dummy {
    #[allow(unused_variables)]
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        Ok("".to_owned())
    }
}

pub struct Version {
    version: Vec<String>,
}

impl Version {
    pub fn new(version: &str) -> Version {
        Version {
            version: version.split('.').map(|s| s.to_owned()).collect::<Vec<_>>(),
        }
    }
}

impl DirectiveHandler for Version {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
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
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        let mut title = self.title.to_owned();
        let raw_body = match args.len() {
            1 => evaluator.evaluate(&args[0]),
            2 => {
                title = evaluator.evaluate(&args[0]);
                evaluator.evaluate(&args[1])
            }
            _ => return Err(()),
        };

        let (body, _) = evaluator.markdown.render(&raw_body, &evaluator.highlighter);
        Ok(format!(
            "<div class=\"admonition admonition-{}\"><span class=\"admonition-title admonition-title-{}\">{}</span>{}</div>\n",
            self.class,
            self.class,
            title,
            body
        ))
    }
}

pub struct Concat;

impl DirectiveHandler for Concat {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        Ok(
            args.iter()
                .map(|node| evaluator.evaluate(node))
                .fold(String::new(), |r, c| r + &c),
        )
    }
}

pub struct Markdown;

impl DirectiveHandler for Markdown {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        let body = args.iter()
            .map(|node| evaluator.evaluate(node))
            .fold(String::new(), |r, c| r + &c);

        let (rendered, title) = evaluator.markdown.render(&body, &evaluator.highlighter);

        if !evaluator.theme_config.contains_key("title") {
            evaluator
                .theme_config
                .insert("title".to_owned(), serde_json::Value::String(title));
        }

        let rendered = rendered.trim().to_owned();
        Ok(rendered)
    }
}

pub struct Template {
    template: String,
    checkers: Vec<Regex>,
}

impl Template {
    pub fn new(template: String, checkers: Vec<Regex>) -> Self {
        Template { template, checkers }
    }
}

impl DirectiveHandler for Template {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        let checkers = self.checkers.iter().map(Some).chain(iter::repeat(None));

        let args: Result<Vec<String>, ()> = args.iter()
            .map(|node| match node.value {
                NodeValue::Owned(ref s) => s.to_owned(),
                NodeValue::Children(_) => evaluator.evaluate(node),
            })
            .chain(iter::repeat("".to_owned()))
            .zip(checkers)
            .map(|(arg, checker)| match checker {
                Some(checker) => if checker.is_match(&arg) {
                    Ok(arg)
                } else {
                    Err(())
                },
                _ => Ok(arg),
            })
            .take(cmp::max(args.len(), self.checkers.len()))
            .collect();

        let args = match args {
            Ok(args) => args,
            Err(_) => return Err(()),
        };

        lazy_static! {
            static ref RE: Regex = Regex::new(r#"\$\{(\d)\}"#).unwrap();
        }

        let result = RE.replace_all(&self.template, |captures: &Captures| {
            let n = str::parse::<usize>(&captures[1]).expect("Failed to parse template number");
            match args.get(n) {
                Some(s) => s.to_owned(),
                None => "".to_owned(),
            }
        });

        Ok(result.into_owned())
    }
}

pub struct DefineTemplate;

impl DirectiveHandler for DefineTemplate {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let name = match consume_string(&mut iter, evaluator) {
            Some(s) => s,
            None => return Err(()),
        };

        let template_text = match consume_string(&mut iter, evaluator) {
            Some(s) => s,
            None => return Err(()),
        };

        let checkers: Result<Vec<Regex>, ()> = iter.map(|node| {
            let pattern_string = match node.value {
                NodeValue::Owned(ref s) => s.to_owned(),
                NodeValue::Children(_) => evaluator.evaluate(node),
            };

            Regex::new(&pattern_string).or(Err(()))
        }).collect();

        let checkers = match checkers {
            Ok(c) => c,
            Err(_) => return Err(()),
        };

        evaluator.register(name, Box::new(Template::new(template_text, checkers)));
        Ok("".to_owned())
    }
}

pub struct DefinitionList;

impl DirectiveHandler for DefinitionList {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        let segments: Result<Vec<_>, _> = args.iter()
            .map(|node| match node.value {
                NodeValue::Owned(_) => Err(()),
                NodeValue::Children(ref children) => {
                    if children.len() != 2 {
                        return Err(());
                    }

                    let term = evaluator.evaluate(&children[0]);
                    let body = evaluator.evaluate(&children[1]);
                    let (definition, _) = evaluator.markdown.render(&body, &evaluator.highlighter);
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

impl DirectiveHandler for Include {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        if args.len() != 1 {
            return Err(());
        }

        let mut path = PathBuf::from(evaluator.evaluate(&args[0]));
        if !path.is_absolute() {
            let prefix = evaluator
                .parser
                .get_node_source_path(&args[0])
                .expect("Node with unknown file ID")
                .parent()
                .unwrap_or_else(|| Path::new(""));
            path = prefix.join(path.to_owned());
        }

        let node = match evaluator.parser.parse(path.as_ref()) {
            Ok(n) => n,
            Err(_) => return Err(()),
        };

        Ok(evaluator.evaluate(&node))
    }
}

pub struct Import;

impl DirectiveHandler for Import {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        let include = Include;
        include.handle(evaluator, args)?;

        Ok("".to_owned())
    }
}

pub struct Let;

impl DirectiveHandler for Let {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        if args.len() < 1 {
            return Err(());
        }

        let mut variables = Vec::new();
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
                    let evaluated_value = Rc::new(StoredValue::Node(
                        Node::new_string(evaluator.evaluate(&pair[1])),
                    ));

                    let mut entry = evaluator.ctx.entry(evaluated_key.to_owned());
                    let original_value = match entry {
                        Entry::Occupied(mut slot) => {
                            Some(mem::replace(slot.get_mut(), evaluated_value))
                        }
                        Entry::Vacant(slot) => {
                            slot.insert(evaluated_value);
                            None
                        }
                    };

                    variables.push((evaluated_key, original_value));
                }
            }
        }

        let concat = Concat;
        let result = concat.handle(evaluator, &args[1..]);

        for (key, original_value) in variables {
            match original_value {
                Some(value) => evaluator.ctx.insert(key, value),
                None => evaluator.ctx.remove(&key),
            };
        }

        result
    }
}

pub struct Define;

impl DirectiveHandler for Define {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        if args.len() != 2 {
            return Err(());
        }

        let key = evaluator.evaluate(&args[0]);
        let value = Rc::new(StoredValue::Node(
            Node::new(args[1].value.clone(), args[1].file_id),
        ));

        evaluator.ctx.insert(key.to_owned(), value);
        Ok("".to_owned())
    }
}

pub struct ThemeConfig;

impl DirectiveHandler for ThemeConfig {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        if args.len() % 2 != 0 {
            return Err(());
        }

        for pair in args.chunks(2) {
            let key = evaluator.evaluate(&pair[0]);
            let value = evaluator.evaluate(&pair[1]);

            evaluator
                .theme_config
                .insert(key, serde_json::Value::String(value));
        }

        Ok("".to_owned())
    }
}

pub struct TocTree;

impl DirectiveHandler for TocTree {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        let current_slug = evaluator.get_slug().to_owned();

        for arg in args {
            match arg.value {
                NodeValue::Owned(ref slug) => {
                    evaluator
                        .toctree
                        .add(&current_slug, Slug::new(slug.to_owned()), None);
                }
                NodeValue::Children(ref children) => {
                    if children.len() != 2 {
                        return Err(());
                    }

                    let title = evaluator.evaluate(&children[0]);
                    let slug = evaluator.evaluate(&children[1]);

                    evaluator
                        .toctree
                        .add(&current_slug, Slug::new(slug), Some(title));
                }
            }
        }

        Ok(String::new())
    }
}

pub struct Heading {
    level: &'static str,
}

impl Heading {
    pub fn new(level: u8) -> Self {
        let level = match level {
            1 => "#",
            2 => "##",
            3 => "###",
            4 => "####",
            5 => "#####",
            6 => "######",
            _ => panic!("Unknown heading level"),
        };

        Heading { level }
    }
}

impl DirectiveHandler for Heading {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let arg1 = match consume_string(&mut iter, evaluator) {
            Some(t) => t,
            None => return Err(()),
        };

        let arg2 = consume_string(&mut iter, evaluator);

        match arg2 {
            Some(title) => {
                let refdef = RefDef::new(&title, evaluator.get_slug());
                evaluator.refdefs.insert(arg1, refdef);
                Ok(format!("\n{} {}\n", self.level, title))
            }
            None => Ok(format!("\n{} {}\n", self.level, arg1)),
        }
    }
}

pub struct RefDefDirective;

impl DirectiveHandler for RefDefDirective {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let id = match consume_string(&mut iter, evaluator) {
            Some(t) => t,
            None => return Err(()),
        };

        let title = match consume_string(&mut iter, evaluator) {
            Some(t) => t,
            None => return Err(()),
        };

        let refdef = RefDef::new(&title, evaluator.get_slug());
        evaluator.refdefs.insert(id, refdef);

        Ok(String::new())
    }
}

pub struct RefDirective;

impl DirectiveHandler for RefDirective {
    fn handle(&self, evaluator: &mut Evaluator, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let refid = match consume_string(&mut iter, evaluator) {
            Some(t) => t,
            None => return Err(()),
        };

        let title = match consume_string(&mut iter, evaluator) {
            Some(t) => t,
            None => evaluator.get_placeholder(refid.to_owned(), PlaceholderAction::Title),
        };

        let placeholder = evaluator.get_placeholder(refid, PlaceholderAction::Path);

        Ok(format!("[{}]({})", title, placeholder))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dummy() {
        let mut evaluator = Evaluator::new();
        let handler = Dummy;

        assert_eq!(handler.handle(&mut evaluator, &[]), Ok("".to_owned()));
        assert_eq!(
            handler.handle(&mut evaluator, &[Node::new_string("")]),
            Ok("".to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut evaluator,
                &[Node::new_children(vec![Node::new_string("")])]
            ),
            Ok("".to_owned())
        );
    }

    #[test]
    fn test_version() {
        let mut evaluator = Evaluator::new();
        evaluator.register("concat", Box::new(Concat));
        let handler = Version::new("3.4.0");

        assert_eq!(handler.handle(&mut evaluator, &[]), Ok("3.4.0".to_owned()));
        assert_eq!(
            handler.handle(&mut evaluator, &[Node::new_string("")]),
            Ok("".to_owned())
        );
        assert_eq!(
            handler.handle(&mut evaluator, &[Node::new_string("x")]),
            Ok("3".to_owned())
        );
        assert_eq!(
            handler.handle(&mut evaluator, &[Node::new_string("x.y")]),
            Ok("3.4".to_owned())
        );

        assert_eq!(
            handler.handle(
                &mut evaluator,
                &[
                    Node::new_children(vec![
                        Node::new_string("concat"),
                        Node::new_string("3."),
                        Node::new_string("4"),
                    ])
                ]
            ),
            Ok("3.4".to_owned())
        );
    }

    #[test]
    fn test_admonition() {
        let mut evaluator = Evaluator::new();
        let handler = Admonition::new("note", "Note");

        assert!(handler.handle(&mut evaluator, &[]).is_err());
        assert!(
            handler
                .handle(&mut evaluator, &[Node::new_string("foo")])
                .is_ok()
        );
    }

    #[test]
    fn test_concat() {
        let mut evaluator = Evaluator::new();
        evaluator.register("version", Box::new(Version::new("3.4")));
        let handler = Concat;

        assert_eq!(handler.handle(&mut evaluator, &[]), Ok("".to_owned()));
        assert_eq!(
            handler.handle(&mut evaluator, &[Node::new_string("foo")]),
            Ok("foo".to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut evaluator,
                &[
                    Node::new_string("foo"),
                    Node::new_string("bar"),
                    Node::new_string("baz")
                ]
            ),
            Ok("foobarbaz".to_owned())
        );

        assert_eq!(
            handler.handle(
                &mut evaluator,
                &[
                    Node::new_children(vec![Node::new_string("version")]),
                    Node::new_string("-test")
                ]
            ),
            Ok("3.4-test".to_owned())
        );
    }

    #[test]
    fn test_markdown() {
        let mut evaluator = Evaluator::new();
        let handler = Markdown;

        assert_eq!(handler.handle(&mut evaluator, &[]), Ok("".to_owned()));
        assert_eq!(
            handler.handle(&mut evaluator, &[Node::new_string("Some *markdown* text")]),
            Ok("<p>Some <em>markdown</em> text</p>".to_owned())
        );
    }

    #[test]
    fn test_template() {
        let mut evaluator = Evaluator::new();
        let handler = Template::new(
            r#"[${0}](https://foxquill.com${1} "${2}")"#.to_owned(),
            vec![Regex::new("^.+$").unwrap(), Regex::new("^/.*$").unwrap()],
        );

        assert!(handler.handle(&mut evaluator, &[]).is_err());
        assert_eq!(handler.handle(&mut evaluator, &[
            Node::new_string("SIMD.js Rectangle Intersection"),
            Node::new_string("/simd-rectangle-intersection/")]),
                   Ok(r#"[SIMD.js Rectangle Intersection](https://foxquill.com/simd-rectangle-intersection/ "")"#.to_owned()));
    }

    #[test]
    fn test_let() {
        let mut evaluator = Evaluator::new();
        let handler = Let;

        evaluator.register("concat", Box::new(Concat));

        assert!(handler.handle(&mut evaluator, &[]).is_err());
        let result = handler.handle(
            &mut evaluator,
            &[
                Node::new_children(vec![
                    Node::new_string("foo"),
                    Node::new_children(vec![
                        Node::new_string("concat"),
                        Node::new_string("1"),
                        Node::new_string("2"),
                    ]),
                    Node::new_string("bar"),
                    Node::new_string("3"),
                ]),
                Node::new_children(vec![Node::new_string("foo")]),
                Node::new_children(vec![Node::new_string("bar")]),
            ],
        );

        assert_eq!(result, Ok("123".to_owned()));
    }

    #[test]
    fn test_define() {
        let mut evaluator = Evaluator::new();
        evaluator.register("concat", Box::new(Concat));
        let handler = Define;

        assert_eq!(
            handler.handle(
                &mut evaluator,
                &[Node::new_string("foo"), Node::new_string("foo")]
            ),
            Ok("".to_owned())
        );

        assert!(handler.handle(&mut evaluator, &[]).is_err());
        assert_eq!(
            handler.handle(
                &mut evaluator,
                &[
                    Node::new_string("x"),
                    Node::new_children(vec![
                        Node::new_string("concat"),
                        Node::new_children(vec![
                            Node::new_string("foo"),
                        ]),
                        Node::new_string("bar"),
                    ])
                ]
            ),
            Ok("".to_owned())
        );

        assert_eq!(
            handler.handle(
                &mut evaluator,
                &[Node::new_string("foo"), Node::new_string("bar")]
            ),
            Ok("".to_owned())
        );

        assert_eq!(
            evaluator
                .lookup(&Node::new_string(""), "x", &vec![])
                .unwrap(),
            "barbar".to_owned()
        );

        assert_eq!(
            evaluator
                .lookup(&Node::new_string(""), "foo", &vec![])
                .unwrap(),
            "bar".to_owned()
        );
    }

    #[test]
    fn test_theme_config() {
        let mut evaluator = Evaluator::new();
        let handler = ThemeConfig;

        assert_eq!(handler.handle(&mut evaluator, &[]), Ok("".to_owned()));
        assert_eq!(
            handler.handle(
                &mut evaluator,
                &[Node::new_string("foo"), Node::new_string("bar")]
            ),
            Ok("".to_owned())
        );
        assert_eq!(
            evaluator.theme_config.get("foo"),
            Some(&serde_json::Value::String("bar".to_owned()))
        );
    }

    #[test]
    fn test_heading() {
        let mut evaluator = Evaluator::new();
        evaluator.set_slug(Slug::new("index".to_owned()));
        let handler = Heading::new(2);

        assert!(handler.handle(&mut evaluator, &[]).is_err());
        assert_eq!(
            handler.handle(
                &mut evaluator,
                &[Node::new_string("a-title"), Node::new_string("A Title")]
            ),
            Ok("\n## A Title\n".to_owned())
        );
        assert_eq!(
            evaluator.refdefs.get("a-title").unwrap().title,
            "A Title".to_owned()
        );
    }

    #[test]
    fn test_refdef() {
        let mut evaluator = Evaluator::new();
        evaluator.set_slug(Slug::new("index".to_owned()));
        let handler = RefDefDirective;

        assert!(handler.handle(&mut evaluator, &[]).is_err());
        assert!(
            handler
                .handle(&mut evaluator, &[Node::new_string("a-title")])
                .is_err()
        );
        assert_eq!(
            handler.handle(
                &mut evaluator,
                &[Node::new_string("a-title"), Node::new_string("A Title")]
            ),
            Ok(String::new())
        );
        assert_eq!(
            evaluator.refdefs.get("a-title").unwrap().title,
            "A Title".to_owned()
        );
    }
}
