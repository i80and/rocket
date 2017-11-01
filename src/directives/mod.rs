use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::sync::Arc;
use std::{cmp, iter, mem, slice, str};
use regex::{Captures, Regex};
use serde_json;
use parse::{Node, NodeValue};
use page::Slug;
use evaluator::{PlaceholderAction, RefDef, StoredValue, Worker};

pub mod logic;
pub mod glossary;

fn consume_string(iter: &mut slice::Iter<Node>, worker: &mut Worker) -> Option<String> {
    match iter.next() {
        Some(n) => match n.value {
            NodeValue::Owned(ref s) => Some(s.to_owned()),
            NodeValue::Children(_) => Some(worker.evaluate(n)),
        },
        None => None,
    }
}

pub fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("&#34;"),
            '\'' => result.push_str("&#39;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '&' => result.push_str("&amp;"),
            _ => result.push(ch),
        }
    }

    result
}

pub fn concat_nodes(
    iter: &mut slice::Iter<Node>,
    worker: &mut Worker,
    sep: &'static str,
) -> String {
    iter.map(|node| worker.evaluate(node))
        .fold(String::new(), |r, c| if r.is_empty() {
            c
        } else {
            r + sep + &c
        })
}

pub trait DirectiveHandler {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()>;
}

pub struct Dummy;

impl DirectiveHandler for Dummy {
    #[allow(unused_variables)]
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        Ok("".to_owned())
    }
}

pub struct Code;

impl DirectiveHandler for Code {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let language = consume_string(&mut iter, worker).ok_or(())?;
        let literal = concat_nodes(&mut iter, worker, "");
        let trimmed = literal.trim();

        worker
            .highlighter
            .highlight(&language, trimmed)
            .ok()
            .ok_or(())
    }
}

pub struct Version {
    version: Vec<String>,
}

impl Version {
    pub fn new(version: &str) -> Self {
        Version {
            version: version.split('.').map(|s| s.to_owned()).collect::<Vec<_>>(),
        }
    }
}

impl DirectiveHandler for Version {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        match args.len() {
            0 => Ok(self.version.join(".")),
            1 => {
                let arg = worker.evaluate(&args[0]);
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
    pub fn new(title: &str, class: &str) -> Self {
        Admonition {
            title: title.to_owned(),
            class: class.to_owned(),
        }
    }
}

impl DirectiveHandler for Admonition {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut title = self.title.to_owned();
        let raw_body = match args.len() {
            1 => worker.evaluate(&args[0]),
            2 => {
                title = worker.evaluate(&args[0]);
                worker.evaluate(&args[1])
            }
            _ => return Err(()),
        };

        Ok(format!(
            concat!(
                "<div class=\"admonition admonition-{}\">",
                "<span class=\"admonition-title admonition-title-{}\">",
                "{}</span>",
                "{}</div>\n"
            ),
            self.class,
            self.class,
            title,
            &raw_body
        ))
    }
}

pub struct Concat;

impl DirectiveHandler for Concat {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        Ok(concat_nodes(&mut iter, worker, ""))
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
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let checkers = self.checkers.iter().map(Some).chain(iter::repeat(None));

        let args: Result<Vec<String>, ()> = args.iter()
            .map(|node| match node.value {
                NodeValue::Owned(ref s) => s.to_owned(),
                NodeValue::Children(_) => worker.evaluate(node),
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
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let name = consume_string(&mut iter, worker).ok_or(())?;
        let template_text = consume_string(&mut iter, worker).ok_or(())?;

        let checkers: Result<Vec<Regex>, ()> = iter.map(|node| {
            let pattern_string = match node.value {
                NodeValue::Owned(ref s) => s.to_owned(),
                NodeValue::Children(_) => worker.evaluate(node),
            };

            Regex::new(&pattern_string).or(Err(()))
        }).collect();

        let checkers = match checkers {
            Ok(c) => c,
            Err(_) => return Err(()),
        };

        worker.register(name, Box::new(Template::new(template_text, checkers)));
        Ok("".to_owned())
    }
}

pub struct DefinitionList;

impl DirectiveHandler for DefinitionList {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let segments: Result<Vec<_>, _> = args.iter()
            .map(|node| match node.value {
                NodeValue::Owned(_) => Err(()),
                NodeValue::Children(ref children) => {
                    if children.len() != 2 {
                        return Err(());
                    }

                    let term = worker.evaluate(&children[0]);
                    let body = worker.evaluate(&children[1]);
                    Ok(format!("<dt>{}</dt><dd>{}</dd>", term, &body))
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
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        if args.len() != 1 {
            return Err(());
        }

        let path = worker.evaluate(&args[0]);
        let path = worker.get_source_path(&args[0], &path);
        let node = match worker.parser.parse(path.as_ref()) {
            Ok(n) => n,
            Err(msg) => {
                let msg = format!("Failed to parse '{}': {}", path.to_string_lossy(), msg);
                worker.error(&args[0], &msg);
                return Err(());
            }
        };

        Ok(worker.evaluate(&node))
    }
}

pub struct Import;

impl DirectiveHandler for Import {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let include = Include;
        include.handle(worker, args)?;

        Ok("".to_owned())
    }
}

pub struct Let;

impl DirectiveHandler for Let {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
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
                    let evaluated_key = worker.evaluate(&pair[0]);
                    let evaluated_value = Arc::new(StoredValue::Node(Node::new_string(
                        worker.evaluate(&pair[1]),
                        pair[1].file_id,
                        pair[1].lineno,
                    )));

                    let entry = worker.ctx.entry(evaluated_key.to_owned());
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
        let result = concat.handle(worker, &args[1..]);

        for (key, original_value) in variables {
            match original_value {
                Some(value) => worker.ctx.insert(key, value),
                None => worker.ctx.remove(&key),
            };
        }

        result
    }
}

pub struct Define;

impl DirectiveHandler for Define {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let arg1 = consume_string(&mut iter, worker).ok_or(())?;
        let arg2 = iter.next().ok_or(())?;
        let arg3 = iter.next();

        if iter.next().is_some() {
            return Err(());
        }

        let (eager, key, value_node) = match arg3 {
            Some(value) => {
                if arg1 != "evaluate" {
                    return Err(());
                }

                (true, worker.evaluate(arg2), value)
            }
            None => (false, arg1, arg2),
        };

        let value = if eager {
            let evaluated = worker.evaluate(value_node);
            Node::new(
                NodeValue::Owned(evaluated),
                value_node.file_id,
                value_node.lineno,
            )
        } else {
            Node::new(
                value_node.value.clone(),
                value_node.file_id,
                value_node.lineno,
            )
        };

        worker
            .ctx
            .insert(key.to_owned(), Arc::new(StoredValue::Node(value)));
        Ok("".to_owned())
    }
}

pub struct ThemeConfig;

impl DirectiveHandler for ThemeConfig {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        if args.len() % 2 != 0 {
            return Err(());
        }

        for pair in args.chunks(2) {
            let key = worker.evaluate(&pair[0]);
            let value = worker.evaluate(&pair[1]);

            worker
                .theme_config
                .insert(key, serde_json::Value::String(value));
        }

        Ok("".to_owned())
    }
}

pub struct TocTree;

impl DirectiveHandler for TocTree {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        for arg in args {
            match arg.value {
                NodeValue::Owned(ref slug) => {
                    worker.add_to_toctree(Slug::new(slug.to_owned()), None);
                }
                NodeValue::Children(ref children) => {
                    if children.len() != 2 {
                        return Err(());
                    }

                    let title = worker.evaluate(&children[0]);
                    let slug = worker.evaluate(&children[1]);

                    worker.add_to_toctree(Slug::new(slug), Some(title));
                }
            }
        }

        Ok(String::new())
    }
}

pub struct Heading {
    level: i8,
}

impl Heading {
    pub fn new(level: i8) -> Self {
        Heading { level }
    }

    fn title_to_id(title: &str) -> String {
        let mut result = String::with_capacity(title.len());

        for c in title.chars() {
            if c.is_alphanumeric() {
                result.extend(c.to_lowercase());
            } else if c == '-' || c == '_' {
                result.push(c);
            } else if c == ' ' {
                result.push('-');
            } else {
                result.push_str(&(c as u32).to_string());
            }
        }

        result
    }
}

impl DirectiveHandler for Heading {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let arg1 = consume_string(&mut iter, worker).ok_or(())?;
        let arg2 = consume_string(&mut iter, worker);

        let (title, refdef) = match arg2 {
            Some(title) => {
                let refdef = RefDef::new(&title, worker.get_slug());
                worker.insert_refdef(arg1.to_owned(), refdef);
                (title, arg1)
            }
            None => {
                let title_id = Self::title_to_id(&arg1);
                (arg1, title_id)
            }
        };

        if !worker.theme_config.contains_key("title") {
            worker.theme_config.insert(
                "title".to_owned(),
                serde_json::Value::String(title.to_owned()),
            );
        }

        let prefix = worker.handle_heading(self.level)?;

        Ok(format!(
            r#"{}<h{} id="{}">{}</h{}>"#,
            prefix,
            self.level,
            escape_string(&refdef),
            title,
            self.level
        ))
    }
}

pub struct RefDefDirective;

impl DirectiveHandler for RefDefDirective {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let id = consume_string(&mut iter, worker).ok_or(())?;
        let title = consume_string(&mut iter, worker).ok_or(())?;

        let refdef = RefDef::new(&title, worker.get_slug());
        worker.insert_refdef(id, refdef);

        Ok(String::new())
    }
}

pub struct RefDirective;

impl DirectiveHandler for RefDirective {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let refid = consume_string(&mut iter, worker).ok_or(())?;

        let title = match consume_string(&mut iter, worker) {
            Some(t) => t,
            None => worker.get_placeholder(refid.to_owned(), PlaceholderAction::Title),
        };

        let placeholder = worker.get_placeholder(refid, PlaceholderAction::Path);

        Ok(format!(r#"<a href="{}">{}</a>"#, placeholder, title))
    }
}

pub struct Steps;

impl DirectiveHandler for Steps {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut result: Vec<Cow<str>> = Vec::with_capacity(2 + (args.len() * 4));
        result.push(Cow::from(r#"<div class="steps">"#));

        for (i, step_node) in args.iter().enumerate() {
            let parse_args = |args: &[Node], worker: &mut Worker| {
                if args.len() != 3 {
                    return Err(());
                }

                Ok((worker.evaluate(&args[1]), worker.evaluate(&args[2])))
            };

            let (title, body) = match step_node.value {
                NodeValue::Owned(ref s) => {
                    let stored_value = match worker.ctx.get(s) {
                        Some(v) => Arc::clone(v),
                        None => return Err(()),
                    };

                    match *stored_value {
                        StoredValue::Node(ref node) => match node.value {
                            NodeValue::Owned(_) => return Err(()),
                            NodeValue::Children(ref children) => parse_args(children, worker),
                        },
                        _ => return Err(()),
                    }
                }
                NodeValue::Children(ref children) => parse_args(children, worker),
            }?;

            result.push(Cow::from(concat!(
                r#"<div class="steps__step">"#,
                r#"<div class="steps__bullet">"#,
                r#"<div class="steps__stepnumber">"#
            )));
            result.push(Cow::from((i + 1).to_string()));
            result.push(Cow::from(r#"</div></div>"#));
            result.push(Cow::from(
                format!(r#"<h4>{}</h4><div>{}</div></div>"#, title, body),
            ))
        }

        result.push(Cow::from("</div>"));
        Ok(result.concat())
    }
}

pub struct Figure;

impl DirectiveHandler for Figure {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let src = escape_string(&consume_string(&mut iter, worker).ok_or(())?);
        let src = worker.add_asset(&src)?;
        let alt = escape_string(&consume_string(&mut iter, worker).ok_or(())?);

        let width = consume_string(&mut iter, worker);
        let width_term = match width {
            Some(ref s) => {
                let width_integer = s.parse::<u16>().ok().ok_or(())?;
                Cow::from(format!(" width={}px", width_integer))
            }
            None => Cow::from(""),
        };

        Ok(format!(
            r#"<img src="{}" alt="{}"{}>"#,
            src,
            alt,
            width_term
        ))
    }
}

pub struct FormattingMarker {
    tag: &'static str,
}

impl FormattingMarker {
    pub fn new(tag: &'static str) -> Self {
        Self { tag }
    }
}

impl DirectiveHandler for FormattingMarker {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let body = concat_nodes(&mut iter, worker, " ");
        Ok(format!("<{}>{}</{}>", self.tag, body, self.tag))
    }
}

pub struct Link;

impl DirectiveHandler for Link {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let href = consume_string(&mut iter, worker).ok_or(())?;
        let href = escape_string(&href);
        let body = concat_nodes(&mut iter, worker, " ");
        let b = if body.is_empty() { &href } else { &body };
        Ok(format!(r#"<a href="{}">{}</a>"#, href, b))
    }
}

pub struct List {
    tag: &'static str,
}

impl List {
    pub fn new(tag: &'static str) -> Self {
        Self { tag }
    }
}

impl DirectiveHandler for List {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let body: Vec<String> = args.iter()
            .map(|node| {
                let item_body = worker.evaluate(node);
                format!("<li>{}</li>", item_body)
            })
            .collect();

        Ok(format!(
            "<{}>{}</{}>",
            self.tag,
            body.as_slice().concat(),
            self.tag
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evaluator::Evaluator;

    fn node_string(s: &str) -> Node {
        Node::new_string(s, 0, -1)
    }

    fn node_children(nodes: Vec<Node>) -> Node {
        Node::new_children(nodes, 0, -1)
    }

    #[test]
    fn test_dummy() {
        let mut evaluator = Evaluator::new();
        let mut worker = Worker::new(&mut evaluator);

        let handler = Dummy;

        assert_eq!(handler.handle(&mut worker, &[]), Ok("".to_owned()));
        assert_eq!(
            handler.handle(&mut worker, &[node_string("")]),
            Ok("".to_owned())
        );
        assert_eq!(
            handler.handle(&mut worker, &[node_children(vec![node_string("")])]),
            Ok("".to_owned())
        );
    }

    #[test]
    fn test_version() {
        let mut evaluator = Evaluator::new();
        let mut worker = Worker::new(&mut evaluator);
        worker.register("concat", Box::new(Concat));
        let handler = Version::new("3.4.0");

        assert_eq!(handler.handle(&mut worker, &[]), Ok("3.4.0".to_owned()));
        assert_eq!(
            handler.handle(&mut worker, &[node_string("")]),
            Ok("".to_owned())
        );
        assert_eq!(
            handler.handle(&mut worker, &[node_string("x")]),
            Ok("3".to_owned())
        );
        assert_eq!(
            handler.handle(&mut worker, &[node_string("x.y")]),
            Ok("3.4".to_owned())
        );

        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_children(vec![
                        node_string("concat"),
                        node_string("3."),
                        node_string("4"),
                    ])
                ]
            ),
            Ok("3.4".to_owned())
        );
    }

    #[test]
    fn test_admonition() {
        let mut evaluator = Evaluator::new();
        let mut worker = Worker::new(&mut evaluator);
        let handler = Admonition::new("note", "Note");

        assert!(handler.handle(&mut worker, &[]).is_err());
        assert!(handler.handle(&mut worker, &[node_string("foo")]).is_ok());
    }

    #[test]
    fn test_concat() {
        let mut evaluator = Evaluator::new();
        let mut worker = Worker::new(&mut evaluator);
        worker.register("version", Box::new(Version::new("3.4")));
        let handler = Concat;

        assert_eq!(handler.handle(&mut worker, &[]), Ok("".to_owned()));
        assert_eq!(
            handler.handle(&mut worker, &[node_string("foo")]),
            Ok("foo".to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut worker,
                &[node_string("foo"), node_string("bar"), node_string("baz")]
            ),
            Ok("foobarbaz".to_owned())
        );

        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_children(vec![node_string("version")]),
                    node_string("-test")
                ]
            ),
            Ok("3.4-test".to_owned())
        );
    }

    #[test]
    fn test_template() {
        let mut evaluator = Evaluator::new();
        let mut worker = Worker::new(&mut evaluator);
        let handler = Template::new(
            r#"[${0}](https://foxquill.com${1} "${2}")"#.to_owned(),
            vec![Regex::new("^.+$").unwrap(), Regex::new("^/.*$").unwrap()],
        );

        assert!(handler.handle(&mut worker, &[]).is_err());
        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_string("SIMD.js Rectangle Intersection"),
                    node_string("/simd-rectangle-intersection/")
                ]
            ),
            Ok(
                concat!(
                    "[SIMD.js Rectangle Intersection]",
                    r#"(https://foxquill.com/simd-rectangle-intersection/ "")"#
                ).to_owned()
            )
        );
    }

    #[test]
    fn test_let() {
        let mut evaluator = Evaluator::new();
        let mut worker = Worker::new(&mut evaluator);
        let handler = Let;

        worker.register("concat", Box::new(Concat));

        assert!(handler.handle(&mut worker, &[]).is_err());
        let result = handler.handle(
            &mut worker,
            &[
                node_children(vec![
                    node_string("foo"),
                    node_children(vec![
                        node_string("concat"),
                        node_string("1"),
                        node_string("2"),
                    ]),
                    node_string("bar"),
                    node_string("3"),
                ]),
                node_children(vec![node_string("foo")]),
                node_children(vec![node_string("bar")]),
            ],
        );

        assert_eq!(result, Ok("123".to_owned()));
    }

    #[test]
    fn test_define() {
        let mut evaluator = Evaluator::new();
        let mut worker = Worker::new(&mut evaluator);
        worker.register("concat", Box::new(Concat));
        let handler = Define;

        assert_eq!(
            handler.handle(&mut worker, &[node_string("foo"), node_string("foo")]),
            Ok("".to_owned())
        );

        assert!(handler.handle(&mut worker, &[]).is_err());
        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_string("x"),
                    node_children(vec![
                        node_string("concat"),
                        node_children(vec![node_string("foo")]),
                        node_string("bar"),
                    ])
                ]
            ),
            Ok("".to_owned())
        );

        assert_eq!(
            handler.handle(&mut worker, &[node_string("foo"), node_string("bar")]),
            Ok("".to_owned())
        );

        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_string("evaluate"),
                    node_string("eager"),
                    node_children(vec![node_string("x")])
                ]
            ),
            Ok("".to_owned())
        );

        assert_eq!(
            worker.lookup(&node_string(""), "x", &vec![]).unwrap(),
            "barbar".to_owned()
        );

        assert_eq!(
            worker.lookup(&node_string(""), "eager", &vec![]).unwrap(),
            "barbar".to_owned()
        );

        assert_eq!(
            worker.lookup(&node_string(""), "foo", &vec![]).unwrap(),
            "bar".to_owned()
        );

        // Now change foo to make sure x changes but eager does not
        assert_eq!(
            handler.handle(&mut worker, &[node_string("foo"), node_string("baz")]),
            Ok("".to_owned())
        );

        assert_eq!(
            worker.lookup(&node_string(""), "x", &vec![]).unwrap(),
            "bazbar".to_owned()
        );

        assert_eq!(
            worker.lookup(&node_string(""), "eager", &vec![]).unwrap(),
            "barbar".to_owned()
        );
    }

    #[test]
    fn test_theme_config() {
        let mut evaluator = Evaluator::new();
        let mut worker = Worker::new(&mut evaluator);
        let handler = ThemeConfig;

        assert_eq!(handler.handle(&mut worker, &[]), Ok("".to_owned()));
        assert_eq!(
            handler.handle(&mut worker, &[node_string("foo"), node_string("bar")]),
            Ok("".to_owned())
        );
        assert_eq!(
            worker.theme_config.get("foo"),
            Some(&serde_json::Value::String("bar".to_owned()))
        );
    }

    #[test]
    fn test_heading() {
        let mut evaluator = Evaluator::new();
        {
            let mut worker = Worker::new(&mut evaluator);
            worker.set_slug(Slug::new("index".to_owned()));
            let handler = Heading::new(2);

            assert!(handler.handle(&mut worker, &[]).is_err());
            assert!(
                handler
                    .handle(&mut worker, &[node_string("A Title")])
                    .is_err()
            );

            let handler = Heading::new(1);
            assert_eq!(
                handler.handle(
                    &mut worker,
                    &[node_string("a-title"), node_string("A Title")]
                ),
                Ok(r#"<section><h1 id="a-title">A Title</h1>"#.to_owned())
            );

            let handler = Heading::new(2);
            assert_eq!(
                handler.handle(&mut worker, &[node_string("A Second Title")]),
                Ok(r#"<section><h2 id="a-second-title">A Second Title</h2>"#.to_owned())
            );

            let handler = Heading::new(3);
            assert_eq!(
                handler.handle(&mut worker, &[node_string("A Third Title")]),
                Ok(r#"<section><h3 id="a-third-title">A Third Title</h3>"#.to_owned())
            );

            let handler = Heading::new(1);
            assert_eq!(
                handler.handle(&mut worker, &[node_string("A Fourth Title")]),
                Ok(r#"</section></section><h1 id="a-fourth-title">A Fourth Title</h1>"#.to_owned())
            );

            assert_eq!(worker.close_sections(), "</section>".to_owned());
        }

        assert_eq!(
            evaluator
                .refdefs
                .read()
                .unwrap()
                .get("a-title")
                .unwrap()
                .title,
            "A Title".to_owned()
        );
    }

    #[test]
    fn test_refdef() {
        let mut evaluator = Evaluator::new();
        {
            let mut worker = Worker::new(&mut evaluator);
            worker.set_slug(Slug::new("index".to_owned()));
            let handler = RefDefDirective;

            assert!(handler.handle(&mut worker, &[]).is_err());
            assert!(
                handler
                    .handle(&mut worker, &[node_string("a-title")])
                    .is_err()
            );
            assert_eq!(
                handler.handle(
                    &mut worker,
                    &[node_string("a-title"), node_string("A Title")]
                ),
                Ok(String::new())
            );
        }

        assert_eq!(
            evaluator
                .refdefs
                .read()
                .unwrap()
                .get("a-title")
                .unwrap()
                .title,
            "A Title".to_owned()
        );
    }

    #[test]
    fn test_figure() {
        let mut evaluator = Evaluator::new();
        let mut worker = Worker::new(&mut evaluator);
        worker.set_slug(Slug::new("index".to_owned()));
        let handler = Figure;

        assert!(handler.handle(&mut worker, &[]).is_err());
        assert!(
            handler
                .handle(&mut worker, &[node_string("foo.png")])
                .is_err()
        );
        assert_eq!(
            handler.handle(
                &mut worker,
                &[node_string("fo\"o.png"), node_string("al\"t")]
            ),
            Ok(r#"<img src="_static/fo&#34;o.png" alt="al&#34;t">"#.to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_string("fo\"o.png"),
                    node_string("al\"t"),
                    node_string("320")
                ]
            ),
            Ok(r#"<img src="_static/fo&#34;o.png" alt="al&#34;t" width=320px>"#.to_owned())
        );

        worker.set_slug(Slug::new("reference/directives".to_owned()));
        assert_eq!(
            handler.handle(&mut worker, &[node_string("foo.png"), node_string("foo")]),
            Ok(r#"<img src="../../_static/foo.png" alt="foo">"#.to_owned())
        );
    }

    #[test]
    fn test_formatting_marker() {
        let mut evaluator = Evaluator::new();
        let mut worker = Worker::new(&mut evaluator);
        worker.register("concat", Box::new(Concat));

        let handler = FormattingMarker::new("strong");
        assert_eq!(
            handler.handle(&mut worker, &[]),
            Ok(r#"<strong></strong>"#.to_owned())
        );
        assert_eq!(
            handler.handle(&mut worker, &[node_string("foo"), node_string("bar")]),
            Ok(r#"<strong>foo bar</strong>"#.to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_children(vec![
                        node_string("concat"),
                        node_string("1"),
                        node_string("2"),
                    ],),
                    node_string("bar")
                ]
            ),
            Ok(r#"<strong>12 bar</strong>"#.to_owned())
        );
    }

    #[test]
    fn test_link() {
        let mut evaluator = Evaluator::new();
        let mut worker = Worker::new(&mut evaluator);
        worker.register("concat", Box::new(Concat));
        let handler = Link;
        assert!(handler.handle(&mut worker, &[]).is_err());
        assert_eq!(
            handler.handle(&mut worker, &[node_string("https://foxquill.com")]),
            Ok(r#"<a href="https://foxquill.com">https://foxquill.com</a>"#.to_owned())
        );

        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_string("https://foxquill.com"),
                    node_children(vec![
                        node_string("concat"),
                        node_string("foo"),
                        node_string("bar"),
                    ]),
                    node_string("baz")
                ]
            ),
            Ok(r#"<a href="https://foxquill.com">foobar baz</a>"#.to_owned())
        );
    }
}
