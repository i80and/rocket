use std::fs::File;
use std::io::prelude::*;
use std::mem;
use std::path::{Path, PathBuf};
use std::str;
use regex::Regex;

use lex::{lex, Token};

lazy_static! {
    static ref PAT_IS_WHITESPACE: Regex = Regex::new(r#"^\s+$"#).expect("Failed to compile whitespace regex");
}

type FileID = u32;

#[derive(Debug, Clone, PartialEq)]
pub enum NodeValue {
    Owned(String),
    Children(Vec<Node>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub value: NodeValue,
    pub file_id: FileID,
    pub lineno: i32,
}

impl Node {
    pub fn new(value: NodeValue, file_id: FileID, lineno: i32) -> Self {
        Node {
            value,
            file_id,
            lineno,
        }
    }

    pub fn new_children(value: Vec<Node>, file_id: FileID, lineno: i32) -> Self {
        Node {
            value: NodeValue::Children(value),
            file_id,
            lineno,
        }
    }

    pub fn new_string<S: Into<String>>(value: S, file_id: FileID, lineno: i32) -> Self {
        Node {
            value: NodeValue::Owned(value.into()),
            file_id,
            lineno,
        }
    }

    #[allow(dead_code)]
    pub fn print(&self, indent: usize) {
        match self.value {
            NodeValue::Owned(ref s) => {
                println!("{:indent$}{:?}", "", s, indent = indent);
            }
            NodeValue::Children(ref children) => {
                println!("{:indent$}\\", "", indent = indent);
                for child in children {
                    child.print(indent + 2);
                }
            }
        }
    }
}

enum StackRequest {
    None,
    Pop(u8),
    Push(Box<TokenHandler>),
}

trait TokenHandler {
    fn handle_token(&mut self, token: &Token) -> StackRequest;
    fn finish(&mut self) -> Node;
    fn push(&mut self, node: Node);
    fn name(&self) -> &'static str;
}

struct StateRocket {
    root: Vec<Node>,
    buffer: Vec<String>,
    file_id: FileID,
    lineno: i32,
}

impl StateRocket {
    fn new(file_id: FileID, lineno: i32) -> Self {
        StateRocket {
            root: vec![Node::new_string("concat", file_id, lineno)],
            buffer: vec![],
            file_id: file_id,
            lineno,
        }
    }

    fn ensure_string(&mut self) -> &mut String {
        if self.buffer.is_empty() {
            self.buffer.push(String::with_capacity(2));
        }

        self.buffer.last_mut().unwrap()
    }
}

impl TokenHandler for StateRocket {
    fn handle_token(&mut self, token: &Token) -> StackRequest {
        match *token {
            Token::Text(_, s) => {
                self.buffer.push(s.to_owned());
            }
            Token::Quote(_) => {
                self.ensure_string().push('"');
            }
            Token::StartBlock(lineno) => {
                if !self.buffer.is_empty() {
                    self.root
                        .push(Node::new_string(self.buffer.concat(), self.file_id, lineno));
                    self.buffer.clear();
                }
                return StackRequest::Push(Box::new(StateExpression::new(self.file_id, lineno)));
            }
            Token::RightParen => {
                self.ensure_string().push(')');
            }
            Token::Rocket(_) => {
                self.ensure_string().push_str("=>");
            }
            Token::Dedent => {
                // We need to pop both the rocket and the expression that started the rocket
                return StackRequest::Pop(2);
            }
        }

        StackRequest::None
    }

    fn finish(&mut self) -> Node {
        if !self.buffer.is_empty() {
            let string = self.buffer.concat();
            self.root
                .push(Node::new_string(string, self.file_id, self.lineno));
        }

        Node::new_children(
            mem::replace(&mut self.root, vec![]),
            self.file_id,
            self.lineno,
        )
    }

    fn push(&mut self, node: Node) {
        self.root.push(node);
    }

    fn name(&self) -> &'static str {
        "rocket"
    }
}

struct StateExpression {
    root: Vec<Node>,
    file_id: FileID,
    lineno: i32,

    quote: Vec<String>,
    in_quote: bool,
    new_node: bool,
}

impl StateExpression {
    fn new(file_id: FileID, lineno: i32) -> Self {
        StateExpression {
            root: vec![],
            file_id,
            lineno,
            quote: vec![],
            in_quote: false,
            new_node: true,
        }
    }
}

impl TokenHandler for StateExpression {
    fn handle_token(&mut self, token: &Token) -> StackRequest {
        if self.in_quote {
            match *token {
                Token::Text(_, s) => self.quote.push(s.to_owned()),
                Token::Quote(lineno) => {
                    self.root
                        .push(Node::new_string(self.quote.concat(), self.file_id, lineno));

                    self.in_quote = false;
                    self.quote.clear();
                }
                Token::StartBlock(_) => self.quote.push("(:".to_owned()),
                Token::RightParen => self.quote.push(")".to_owned()),
                Token::Rocket(_) => self.quote.push("=>".to_owned()),
                Token::Dedent => (),
            }
            return StackRequest::None;
        }

        match *token {
            Token::Text(lineno, s) => {
                // When in an expression, whitespace only serves to separate tokens.
                if PAT_IS_WHITESPACE.is_match(s) {
                    self.new_node = true;
                } else {
                    let mut new_node = self.new_node;

                    if !new_node {
                        if let Some(last) = self.root.last_mut() {
                            match last.value {
                                NodeValue::Owned(ref mut val) => val.push_str(s),
                                NodeValue::Children(_) => new_node = true,
                            }
                        } else {
                            new_node = true;
                        }
                    }

                    if new_node {
                        self.root
                            .push(Node::new_string(s.to_owned(), self.file_id, lineno));
                    }
                    self.new_node = false;
                }
            }
            Token::Quote(_) => {
                self.in_quote = true;
            }
            Token::StartBlock(lineno) => {
                return StackRequest::Push(Box::new(StateExpression::new(self.file_id, lineno)));
            }
            Token::Rocket(lineno) => {
                return StackRequest::Push(Box::new(StateRocket::new(self.file_id, lineno)));
            }
            Token::RightParen | Token::Dedent => {
                return StackRequest::Pop(1);
            }
        }

        StackRequest::None
    }

    fn finish(&mut self) -> Node {
        Node::new_children(
            mem::replace(&mut self.root, vec![]),
            self.file_id,
            self.lineno,
        )
    }

    fn push(&mut self, node: Node) {
        self.root.push(node);
    }

    fn name(&self) -> &'static str {
        "expression"
    }
}

struct ParseContextStack {
    stack: Vec<Box<TokenHandler>>,
}

impl ParseContextStack {
    fn new(file_id: FileID, lineno: i32) -> Self {
        ParseContextStack {
            stack: vec![
                Box::new(StateRocket {
                    root: vec![Node::new_string("concat", file_id, lineno)],
                    buffer: vec![],
                    file_id: file_id,
                    lineno: lineno,
                }),
            ],
        }
    }

    fn handle(&mut self, token: &Token) {
        match self.stack
            .last_mut()
            .expect("Empty parse stack")
            .handle_token(token)
        {
            StackRequest::Push(handler) => {
                self.stack.push(handler);
            }
            StackRequest::Pop(n) => for _ in 0..n {
                let mut handler = self.stack.pop().expect("Cannot pop last handler");
                (**self.stack.last_mut().expect("Empty parse stack")).push(handler.finish());
            },
            StackRequest::None => (),
        }
    }
}

pub struct Parser {
    file_ids: Vec<PathBuf>,
}

impl Parser {
    pub fn new() -> Self {
        Parser { file_ids: vec![] }
    }

    pub fn get_node_source_path(&self, node: &Node) -> Option<&Path> {
        match self.file_ids.get(node.file_id as usize) {
            Some(p) => Some(p),
            None => None,
        }
    }

    fn parse_string(&mut self, id: FileID, data: String) -> Result<Node, String> {
        let mut stack = ParseContextStack::new(id, 0);
        for token in lex(&data) {
            stack.handle(&token);
        }

        let root = stack.stack.pop().expect("Empty state stack").finish();
        match stack.stack.pop() {
            Some(_) => Err(format!(
                "Unterminated block started on line {}",
                root.lineno
            )),
            None => Ok(root),
        }
    }

    pub fn parse(&mut self, path: &Path) -> Result<Node, String> {
        debug!("Parsing {}", path.to_string_lossy());

        let id = self.file_ids.len() as FileID;
        self.file_ids.push(path.to_owned());

        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(_) => {
                return Err(format!("Failed to open {}", path.to_string_lossy()));
            }
        };
        let mut data = String::new();
        file.read_to_string(&mut data)
            .expect("Failed to read input file");

        self.parse_string(id, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rocket(mut args: Vec<Node>, lineno: i32) -> Node {
        let mut children = vec![Node::new_string("concat", 0, lineno)];
        for arg in args.drain(..) {
            children.push(arg);
        }
        Node::new_children(children, 0, lineno)
    }

    #[test]
    fn test_empty() {
        let mut parser = Parser::new();
        assert_eq!(parser.parse_string(0, "".to_owned()), Ok(rocket(vec![], 0)));
    }

    #[test]
    fn test_complex() {
        let mut parser = Parser::new();
        let src = "(:h1 Rocket)

Rocket is a fast and powerful text markup format.

(:h2 (:ref writing-your-first-project \"Getting Started\"))
(:h2 =>Example)
(:code txt =>
    (:():h1 Rocket)

    Rocket is a fast and powerful text markup format.

    (:():h2 (:():ref writing-your-first-project \"Getting Started\"))

\x20\x20\x20\x20

(:toctree
  \"reference\"
  \"tutorials\")";

        let h1 = Node::new_children(
            vec![
                Node::new_string("h1", 0, 0),
                Node::new_string("Rocket", 0, 0),
            ],
            0,
            0,
        );
        let para1 = Node::new_string(
            "\n\nRocket is a fast and powerful text markup format.\n\n",
            0,
            4,
        );
        let h2_1 = Node::new_children(
            vec![
                Node::new_string("h2", 0, 4),
                Node::new_children(
                    vec![
                        Node::new_string("ref", 0, 4),
                        Node::new_string("writing-your-first-project", 0, 4),
                        Node::new_string("Getting Started", 0, 4),
                    ],
                    0,
                    4,
                ),
            ],
            0,
            4,
        );
        let h2_2 = Node::new_children(
            vec![
                Node::new_string("h2", 0, 5),
                Node::new_string("=>Example", 0, 5),
            ],
            0,
            5,
        );
        let code = Node::new_children(
            vec![
                Node::new_string("code", 0, 6),
                Node::new_string("txt", 0, 6),
                rocket(
                    vec![
                        Node::new_children(vec![Node::new_string("(", 0, 7)], 0, 7),
                        Node::new_string(
                            ":h1 Rocket)\n\nRocket is a fast and powerful text markup format.\n\n",
                            0,
                            11,
                        ),
                        Node::new_children(vec![Node::new_string("(", 0, 11)], 0, 11),
                        Node::new_string(":h2 ", 0, 11),
                        Node::new_children(vec![Node::new_string("(", 0, 11)], 0, 11),
                        Node::new_string(
                            ":ref writing-your-first-project \"Getting Started\"))\n\n",
                            0,
                            6,
                        ),
                    ],
                    6,
                ),
            ],
            0,
            6,
        );
        let toctree = Node::new_children(
            vec![
                Node::new_string("toctree", 0, 15),
                Node::new_string("reference", 0, 16),
                Node::new_string("tutorials", 0, 17),
            ],
            0,
            15,
        );
        let result = rocket(
            vec![
                h1,
                para1,
                h2_1,
                Node::new_string("\n", 0, 5),
                h2_2,
                Node::new_string("\n", 0, 6),
                code,
                Node::new_string("\n\n", 0, 15),
                toctree,
            ],
            0,
        );
        assert_eq!(parser.parse_string(0, src.to_owned()), Ok(result));
    }
}
