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
}

impl Node {
    pub fn new(value: NodeValue, file_id: FileID) -> Self {
        Node { value, file_id }
    }

    pub fn new_children(value: Vec<Node>) -> Self {
        Node {
            value: NodeValue::Children(value),
            file_id: 0,
        }
    }

    pub fn new_string<S: Into<String>>(value: S) -> Self {
        Node {
            value: NodeValue::Owned(value.into()),
            file_id: 0,
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
}

impl StateRocket {
    fn new(file_id: FileID) -> Self {
        StateRocket {
            root: vec![Node::new_string("concat")],
            buffer: vec![],
            file_id: file_id,
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
            Token::Text(s) => {
                self.buffer.push(s.to_owned());
            }
            Token::Character(c) => {
                self.ensure_string().push(c);
            }
            Token::Quote => {
                self.ensure_string().push('"');
            }
            Token::StartBlock => {
                if !self.buffer.is_empty() {
                    self.root.push(Node::new_string(self.buffer.concat()));
                    self.buffer.clear();
                }
                return StackRequest::Push(Box::new(StateExpression::new(self.file_id)));
            }
            Token::RightParen => {
                self.ensure_string().push(')');
            }
            Token::Rocket => {
                self.ensure_string().push_str("=>");
            }
            Token::Indent => panic!("Unexpected indentation token"),
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
            self.root.push(Node::new_string(string));
        }

        Node::new_children(mem::replace(&mut self.root, vec![]))
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

    quote: Vec<String>,
    in_quote: bool,
}

impl StateExpression {
    fn new(file_id: FileID) -> Self {
        StateExpression {
            root: vec![],
            saw_rocket: false,
            file_id: file_id,
            quote: vec![],
            in_quote: false,
        }
    }
}

impl TokenHandler for StateExpression {
    fn handle_token(&mut self, token: &Token) -> StackRequest {
        if self.in_quote {
            match *token {
                Token::Text(s) => self.quote.push(s.to_owned()),
                Token::Character(c) => self.quote.push(c.to_string()),
                Token::Quote => {
                    self.root.push(Node::new_string(self.quote.concat()));

                    self.in_quote = false;
                    self.quote.clear();
                }
                Token::StartBlock => self.quote.push("(:".to_owned()),
                Token::RightParen => self.quote.push(")".to_owned()),
                Token::Rocket => self.quote.push("=>".to_owned()),
                Token::Indent | Token::Dedent => (),
            }
            return StackRequest::None;
        }

        match *token {
            Token::Text(s) => {
                // When in an expression, whitespace only serves to separate tokens.
                if !PAT_IS_WHITESPACE.is_match(s) {
                    self.root.push(Node::new_string(s.to_owned()));
                }
            }
            Token::Character(c) => if !c.is_whitespace() {
                self.root.push(Node::new_string(c.to_string()));
            } else {
                return StackRequest::None;
            },
            Token::Quote => {
                self.in_quote = true;
            }
            Token::StartBlock => {
                return StackRequest::Push(Box::new(StateExpression::new(self.file_id)));
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
                return StackRequest::Push(Box::new(StateRocket::new(self.file_id)));
            },
        }

        if self.saw_rocket {
            panic!("Expected indentation after =>");
        }

        StackRequest::None
    }

    fn finish(&mut self) -> Node {
        Node::new_children(mem::replace(&mut self.root, vec![]))
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
    fn new(file_id: FileID) -> Self {
        ParseContextStack {
            stack: vec![
                Box::new(StateRocket {
                    root: vec![Node::new_string("md")],
                    buffer: vec![],
                    file_id: file_id,
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

    pub fn parse(&mut self, path: &Path) -> Result<Node, ()> {
        debug!("Parsing {}", path.to_string_lossy());

        let id = self.file_ids.len() as FileID;
        self.file_ids.push(path.to_owned());

        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(_) => {
                error!("Failed to open {}", path.to_string_lossy());
                return Err(());
            }
        };
        let mut data = String::new();
        file.read_to_string(&mut data)
            .expect("Failed to read input file");

        let mut stack = ParseContextStack::new(id);
        for token in lex(&data) {
            stack.handle(&token);
        }

        Ok(stack.stack.pop().expect("Empty state stack").finish())
    }
}
