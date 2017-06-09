use parse::Node;
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
                if arg.len() == 0 {
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

        let body = evaluator.render_markdown(&raw_body);
        Ok(format!("<div class=\"admonition admonition-{}\"><span class=\"admonition-title admonition-title-{}\">{}</span>{}</div>\n",
                self.class, self.class, title, body))
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

        let rendered = evaluator.render_markdown(&body).trim().to_owned();
        return Ok(rendered);
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

        return Ok(format!(r#"<a href="{}">{}</a>"#, url, title));
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
            .map(|node| match node {
                     &Node::Owned(_) => {
                         return Err(());
                     }
                     &Node::Children(ref children) => {
                         if children.len() != 2 {
                             return Err(());
                         }

                         let term = evaluator.evaluate(&children[0]);
                         let definition =
                             evaluator.render_markdown(&evaluator.evaluate(&children[1]));
                         Ok(format!("<dt>{}</dt><dd>{}</dd>", term, definition))
                     }
                 }).collect();

        match segments {
            Ok(s) => Ok(s.concat()),
            Err(_) => Err(()),
        }
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
}
