use std::fs::File;
use std::io::prelude::*;
use std::mem;
use std::str;

use lex::{lex, Token};

#[derive(Debug)]
pub enum Value {
    None,
    Owned(String),
}

#[derive(Debug)]
pub struct Node {
    value: Value,
    children: Vec<Node>,
}

impl Node {
    pub fn new() -> Node {
        Node {
            value: Value::None,
            children: vec![]
        }
    }

    pub fn new_string<S: Into<String>>(value: S) -> Node {
        Node {
            value: Value::Owned(value.into()),
            children: vec![]
        }
    }

    pub fn push(&mut self, node: Node) {
        self.children.push(node);
    }

    pub fn print(&self, indent: usize) {
        println!("{:indent$}{}", "", match self.value {
            Value::None => "None".to_owned(),
            Value::Owned(ref s) => format!("{:?}", s),
        }, indent=indent);
        for child in &self.children {
            child.print(indent + 2);
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
    root: Node,
    buffer: Vec<String>,
}

impl StateRocket {
    fn new() -> StateRocket {
        StateRocket {
            root: Node::new(),
            buffer: vec![],
        }
    }

    fn ensure_string<'a>(&'a mut self) -> &'a mut String {
        if self.buffer.is_empty() {
            self.buffer.push(String::with_capacity(2));
        }

        self.buffer.last_mut().unwrap()
    }
}

impl TokenHandler for StateRocket {
    fn handle_token(&mut self, token: &Token) -> StackRequest {
        match token {
            &Token::Text(s) => {
                self.buffer.push(s.to_owned());
            },
            &Token::Character(c) => {
                self.ensure_string().push(c);
            },
            &Token::String(s) => {
                self.buffer.push(s.to_owned());
            },
            &Token::StartBlock => {
                if !self.buffer.is_empty() {
                    self.root.children.push(Node::new_string(self.buffer.concat()));
                    self.buffer.clear();
                }
                return StackRequest::Push(Box::new(StateExpression::new()));
            },
            &Token::RightParen => {
                self.ensure_string().push(')');
            },
            &Token::Rocket => {
                self.ensure_string().push_str("=>");
            },
            &Token::Indent => { panic!("Unexpected indentation token") },
            &Token::Dedent => {
                // We need to pop both the rocket and the expression that started the rocket
                return StackRequest::Pop(2);
            }
        }

        return StackRequest::None;
    }

    fn finish(&mut self) -> Node {
        if !self.buffer.is_empty() {
            let string = self.buffer.concat();
            self.root.children.push(Node::new_string(string));
        }

        mem::replace(&mut self.root, Node::new())
    }

    fn push(&mut self, node: Node) { self.root.push(node); }
    fn name(&self) -> &'static str { "rocket" }
}

struct StateExpression {
    root: Node,
    saw_rocket: bool,
}

impl StateExpression {
    fn new() -> StateExpression {
        StateExpression {
            root: Node::new(),
            saw_rocket: false,
        }
    }
}

impl TokenHandler for StateExpression {
    fn handle_token(&mut self, token: &Token) -> StackRequest {
        match token {
            &Token::Text(s) => {
                self.root.children.push(Node::new_string(s.to_owned()));
            },
            &Token::Character(c) => {
                self.root.children.push(Node::new_string(c.to_string()));
            },
            &Token::String(s) => {
                self.root.children.push(Node::new_string(s.to_owned()));
            },
            &Token::StartBlock => {
                return StackRequest::Push(Box::new(StateExpression::new()));
            },
            &Token::RightParen => {
                return StackRequest::Pop(1);
            },
            &Token::Rocket => {
                self.saw_rocket = true;
                return StackRequest::None;
            },
            &Token::Indent => {
                if self.saw_rocket {
                    self.saw_rocket = false;
                    return StackRequest::Push(Box::new(StateRocket::new()));
                }
            },
            &Token::Dedent => {
                return StackRequest::Pop(1);
            },
        }

        if self.saw_rocket {
            panic!("Expected indentation after =>");
        }

        return StackRequest::None;
    }

    fn finish(&mut self) -> Node {
        mem::replace(&mut self.root, Node::new())
    }

    fn push(&mut self, node: Node) { self.root.push(node); }
    fn name(&self) -> &'static str { "expression" }
}

struct ParseContextStack {
    stack: Vec<Box<TokenHandler>>
}

impl ParseContextStack {
    fn new() -> ParseContextStack {
        ParseContextStack {
            stack: vec![Box::new(StateRocket::new())]
        }
    }

    fn handle(&mut self, token: &Token) {
        match self.stack.last_mut().expect("Empty parse stack").handle_token(token) {
            StackRequest::Push(handler) => { self.stack.push(handler); },
            StackRequest::Pop(n) => {
                for _ in 0..n {
                    let mut handler = self.stack.pop().expect("Cannot pop last handler");
                    (**self.stack.last_mut().expect("Empty parse stack")).push(handler.finish());
                }
            },
            StackRequest::None => ()
        }
    }
}

pub fn parse(path: &str) -> Node {
    let mut file = File::open(&path).expect("Failed to open input file");
    let mut data = String::new();
    file.read_to_string(&mut data).expect("Failed to read input file");

    let mut stack = ParseContextStack::new();
    for token in lex(&data) {
        stack.handle(&token);
    }

    return stack.stack.pop().expect("Empty state stack").finish();
}
