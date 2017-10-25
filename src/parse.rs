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

#[derive(Debug, Clone)]
pub enum NodeValue {
    Owned(String),
    Children(Vec<Node>),
}

#[derive(Debug, Clone)]
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
    Error(&'static str, i32),
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
            Token::Character(_, c) => {
                self.ensure_string().push(c);
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
            Token::Rocket => {
                self.ensure_string().push_str("=>");
            }
            Token::Indent => {
                return StackRequest::Error("Unexpected indentation token", self.lineno)
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
            self.root.push(Node::new_string(string, self.file_id, -1));
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
    saw_rocket: bool,
    file_id: FileID,
    lineno: i32,

    quote: Vec<String>,
    in_quote: bool,
}

impl StateExpression {
    fn new(file_id: FileID, lineno: i32) -> Self {
        StateExpression {
            root: vec![],
            saw_rocket: false,
            file_id,
            lineno,
            quote: vec![],
            in_quote: false,
        }
    }
}

impl TokenHandler for StateExpression {
    fn handle_token(&mut self, token: &Token) -> StackRequest {
        if self.in_quote {
            match *token {
                Token::Text(_, s) => self.quote.push(s.to_owned()),
                Token::Character(_, c) => self.quote.push(c.to_string()),
                Token::Quote(lineno) => {
                    self.root
                        .push(Node::new_string(self.quote.concat(), self.file_id, lineno));

                    self.in_quote = false;
                    self.quote.clear();
                }
                Token::StartBlock(_) => self.quote.push("(:".to_owned()),
                Token::RightParen => self.quote.push(")".to_owned()),
                Token::Rocket => self.quote.push("=>".to_owned()),
                Token::Indent | Token::Dedent => (),
            }
            return StackRequest::None;
        }

        match *token {
            Token::Text(lineno, s) => {
                // When in an expression, whitespace only serves to separate tokens.
                if !PAT_IS_WHITESPACE.is_match(s) {
                    self.root
                        .push(Node::new_string(s.to_owned(), self.file_id, lineno));
                }
            }
            Token::Character(lineno, c) => if !c.is_whitespace() {
                self.root
                    .push(Node::new_string(c.to_string(), self.file_id, lineno));
            } else {
                return StackRequest::None;
            },
            Token::Quote(_) => {
                self.in_quote = true;
            }
            Token::StartBlock(lineno) => {
                return StackRequest::Push(Box::new(StateExpression::new(self.file_id, lineno)));
            }
            Token::Rocket => {
                self.saw_rocket = true;
                return StackRequest::None;
            }
            Token::RightParen | Token::Dedent => {
                return StackRequest::Pop(1);
            }
            Token::Indent => if self.saw_rocket {
                self.saw_rocket = false;
                return StackRequest::Push(Box::new(StateRocket::new(self.file_id, self.lineno)));
            },
        }

        if self.saw_rocket {
            return StackRequest::Error("Expected indentation after =>", self.lineno);
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
    errors: Vec<(&'static str, i32)>,
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
            errors: vec![],
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
            StackRequest::Error(msg, lineno) => self.errors.push((msg, lineno)),
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

        let mut stack = ParseContextStack::new(id, 0);
        for token in lex(&data) {
            stack.handle(&token);
            if !stack.errors.is_empty() {
                let (msg, lineno) = stack.errors[0];
                return Err(format!("{}: line {}", msg, lineno));
            }
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
}
