use std::fs::File;
use std::io::prelude::*;
use std::mem;
use std::path::{Path, PathBuf};
use std::str;
use regex::Regex;

use lex::{lex, Token};

lazy_static! {
    static ref PAT_IS_WHITESPACE: Regex =
        Regex::new(r#"^\s+$"#).expect("Failed to compile whitespace regex");
}

fn push_start_expression_string(s: &mut String, colon_depth: u8) {
    match colon_depth {
        0 => s.push_str("(:"),
        1 => s.push_str("(::"),
        2 => s.push_str("(:::"),
        _ => {
            s.push('(');
            for _ in 0..u16::from(colon_depth) + 1 {
                s.push(':');
            }
        }
    }
}

fn push_end_expression_string(s: &mut String, colon_depth: u8) {
    match colon_depth {
        0 => s.push_str(":)"),
        1 => s.push_str("::)"),
        2 => s.push_str(":::)"),
        _ => {
            for _ in 0..u16::from(colon_depth) + 1 {
                s.push(':');
            }
            s.push(')');
        }
    }
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
    colon_depth: u8,
    root: Vec<Node>,
    buffer: Vec<String>,
    file_id: FileID,
    lineno: i32,
}

impl StateRocket {
    fn new(colon_depth: u8, file_id: FileID, lineno: i32) -> Self {
        StateRocket {
            colon_depth,
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
            Token::StartBlock(lineno, colon_depth) => if colon_depth < self.colon_depth {
                push_start_expression_string(self.ensure_string(), colon_depth);
            } else {
                if !self.buffer.is_empty() {
                    self.root
                        .push(Node::new_string(self.buffer.concat(), self.file_id, lineno));
                    self.buffer.clear();
                }

                return StackRequest::Push(Box::new(
                    StateExpression::new(colon_depth, self.file_id, lineno),
                ));
            },
            Token::RightParen(colon_depth) => {
                push_end_expression_string(self.ensure_string(), colon_depth);
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
    colon_depth: u8,
    root: Vec<Node>,
    file_id: FileID,
    lineno: i32,

    quote: String,
    quote_should_merge: bool,
    in_quote: bool,
    new_node: bool,
}

impl StateExpression {
    fn new(colon_depth: u8, file_id: FileID, lineno: i32) -> Self {
        StateExpression {
            colon_depth,
            root: vec![],
            file_id,
            lineno,
            quote: String::new(),
            quote_should_merge: false,
            in_quote: false,
            new_node: true,
        }
    }

    fn add_text(&mut self, lineno: i32, s: &str) {
        self.quote_should_merge = true;
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

impl TokenHandler for StateExpression {
    fn handle_token(&mut self, token: &Token) -> StackRequest {
        if self.in_quote {
            match *token {
                Token::Text(_, s) => self.quote.push_str(s),
                Token::Quote(lineno) => {
                    let should_add_node = if self.quote_should_merge {
                        if let Some(node) = self.root.last_mut() {
                            match node.value {
                                NodeValue::Owned(ref mut s) => {
                                    s.push_str(&self.quote);
                                    false
                                }
                                _ => true,
                            }
                        } else {
                            true
                        }
                    } else {
                        true
                    };

                    if should_add_node {
                        self.root.push(Node::new_string(
                            self.quote.to_owned(),
                            self.file_id,
                            lineno,
                        ));
                    }

                    self.quote_should_merge = false;
                    self.in_quote = false;
                    self.quote.clear();
                }
                Token::StartBlock(_, colon_depth) => {
                    push_start_expression_string(&mut self.quote, colon_depth);
                }
                Token::RightParen(colon_depth) => {
                    push_end_expression_string(&mut self.quote, colon_depth);
                }
                Token::Rocket(_) => self.quote.push_str("=>"),
                Token::Dedent => (),
            }
            return StackRequest::None;
        }

        match *token {
            Token::Text(lineno, s) => {
                // When in an expression, whitespace only serves to separate tokens.
                if PAT_IS_WHITESPACE.is_match(s) {
                    self.new_node = true;
                    self.quote_should_merge = false;
                } else {
                    self.add_text(lineno, s);
                }
            }
            Token::Quote(_) => self.in_quote = true,
            Token::StartBlock(lineno, colon_depth) => {
                return StackRequest::Push(Box::new(
                    StateExpression::new(colon_depth, self.file_id, lineno),
                ));
            }
            Token::Rocket(lineno) => {
                return StackRequest::Push(Box::new(
                    StateRocket::new(self.colon_depth, self.file_id, lineno),
                ));
            }
            Token::RightParen(colon_depth) => {
                if colon_depth == self.colon_depth {
                    return StackRequest::Pop(1);
                }

                let mut s = String::with_capacity(1 + usize::from(colon_depth));
                push_end_expression_string(&mut s, colon_depth);
                let lineno = self.lineno;
                self.add_text(lineno, &s);
            }
            Token::Dedent => {
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
        if self.in_quote {
            "expression-quote"
        } else {
            "expression"
        }
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
                    colon_depth: 0,
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

    fn parse_string(&mut self, id: FileID, data: &str) -> Result<Node, String> {
        let mut stack = ParseContextStack::new(id, 0);
        for token in lex(data) {
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

        self.parse_string(id, &data)
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
        assert_eq!(parser.parse_string(0, ""), Ok(rocket(vec![], 0)));
    }

    #[test]
    fn test_word_with_quotes() {
        let mut parser = Parser::new();

        assert!(
            parser
                .parse_string(
                    0,
                    r#"(:`` ":)
(:h3 =>
  "Sally":)"#).is_err());

        assert_eq!(
            parser.parse_string(0, r#"(:`` f"oo ba"r:)"#),
            Ok(rocket(
                vec![
                    Node::new_children(
                        vec![
                            Node::new_string("``", 0, 0),
                            Node::new_string("foo bar", 0, 0),
                        ],
                        0,
                        0,
                    ),
                ],
                0
            ))
        );
    }

    #[test]
    fn test_complex() {
        let mut parser = Parser::new();
        let src = "(:h1 Rocket:)

Rocket is a fast and powerful text markup format.

(:h2 (:ref writing-your-first-project \"Getting Started\":):)
(:h2 =>Example:)
(::code txt =>

    (:h1 Rocket:)

    Rocket is a fast and powerful text markup format.

    (:h2 (:ref writing-your-first-project \"Getting Started\":):)

\x20\x20\x20\x20

(:toctree
  \"reference\"
  \"tutorials\":)";

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
                        Node::new_string(
                            concat!(
                                "(:h1 Rocket:)",
                                "\n\nRocket is a fast and powerful text markup format.\n\n",
                                "(:h2 (:ref writing-your-first-project \"Getting Started\":):)\n\n"
                            ),
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
                Node::new_string("toctree", 0, 16),
                Node::new_string("reference", 0, 17),
                Node::new_string("tutorials", 0, 18),
            ],
            0,
            16,
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
                Node::new_string("\n\n", 0, 16),
                toctree,
            ],
            0,
        );
        assert_eq!(parser.parse_string(0, src), Ok(result));
    }

    #[test]
    fn test_unmatched_block() {
        let mut parser = Parser::new();
        assert!(
            parser
                .parse_string(0, r#"(:foo (:bar:)"#)
                .is_err()
        );
    }
}
