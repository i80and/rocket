use regex::Regex;

lazy_static! {
    static ref PAT_TOKENS: Regex = Regex::new(r#"(?xm)
          (?:\(:)
        | (?:=>)
        | "
        | (?:\n\x20*)
        | =
        | \(
        | \)
        | \s+
        | [^\(\)=\s"]+"#).expect("Failed to compile lexer regex");
}

#[derive(Debug, PartialEq)]
pub enum Token<'a> {
    StartBlock,
    RightParen,
    Rocket,
    Indent,
    Dedent,
    Text(&'a str),
    Character(char),
    Quote,
}

pub fn lex<'a>(data: &'a str) -> Vec<Token> {
    let mut tokens: Vec<Token> = vec![];
    let mut indent: Vec<usize> = vec![0];
    let mut start_rocket = false;

    for pat_match in PAT_TOKENS.find_iter(data) {
        let token_text = pat_match.as_str();
        let bytes = token_text.as_bytes();
        let token = match bytes[0] {
            b')' => Token::RightParen,
            b'"' => Token::Quote,
            b'\n' => {
                // If the line is empty, ignore it.
                if data.as_bytes().get(pat_match.end()) == Some(&b'\n') {
                    tokens.push(Token::Character('\n'));
                    continue;
                }

                // Subtract one for the leading newline
                let new_indentation_level = token_text.len() - 1;

                let mut current_indentation_level =
                    *(indent.last().expect("Indentation stack is empty"));
                while new_indentation_level < current_indentation_level {
                    indent.pop();
                    current_indentation_level =
                        *(indent.last().expect("Indentation stack is empty"));
                    tokens.push(Token::Dedent);
                }

                if start_rocket {
                    indent.push(new_indentation_level);
                    tokens.push(Token::Indent);
                    start_rocket = false;
                    Token::Text(&token_text[new_indentation_level..])
                } else {
                    Token::Text(&token_text[new_indentation_level..])
                }
            }
            b'(' => {
                match bytes.get(1) {
                    Some(&b':') => Token::StartBlock,
                    None => Token::Character('('),
                    _ => panic!("Bad character matched: Expected ':' or nothing"),
                }
            }
            b'=' => {
                match bytes.get(1) {
                    Some(&b'>') => {
                        start_rocket = true;
                        Token::Rocket
                    }
                    None => Token::Character('='),
                    _ => panic!("Bad character matched: Expected '>' or nothing"),
                }
            }
            _ => Token::Text(token_text),
        };

        tokens.push(token)
    }

    while indent.len() > 1 {
        tokens.push(Token::Dedent);
        indent.pop();
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        assert_eq!(lex(""), vec![]);
    }

    #[test]
    fn test_expression() {
        assert_eq!(lex(r#"(:foo bar (:a "b c") "baz" )"#),
                   vec![Token::StartBlock,
                        Token::Text("foo"),
                        Token::Text(" "),
                        Token::Text("bar"),
                        Token::Text(" "),
                        Token::StartBlock,
                        Token::Text("a"),
                        Token::Text(" "),
                        Token::Quote,
                        Token::Text("b"),
                        Token::Text(" "),
                        Token::Text("c"),
                        Token::Quote,
                        Token::RightParen,
                        Token::Text(" "),
                        Token::Quote,
                        Token::Text("baz"),
                        Token::Quote,
                        Token::Text(" "),
                        Token::RightParen]);
    }

    #[test]
    fn test_rocket() {
        assert_eq!(lex(r#"
(:note "a title" =>
  stuff 1

  stuff 2

  (:note =>
    more stuff

  closing nested
"#),
                   vec![Token::Text("\n"),
                        Token::StartBlock,
                        Token::Text("note"),
                        Token::Text(" "),
                        Token::Quote,
                        Token::Text("a"),
                        Token::Text(" "),
                        Token::Text("title"),
                        Token::Quote,
                        Token::Text(" "),
                        Token::Rocket,
                        Token::Indent,
                        Token::Text(" "),
                        Token::Text("stuff"),
                        Token::Text(" "),
                        Token::Text("1"),
                        Token::Character('\n'),
                        Token::Text(" "),
                        Token::Text("stuff"),
                        Token::Text(" "),
                        Token::Text("2"),
                        Token::Character('\n'),
                        Token::Text(" "),
                        Token::StartBlock,
                        Token::Text("note"),
                        Token::Text(" "),
                        Token::Rocket,
                        Token::Indent,
                        Token::Text(" "),
                        Token::Text("more"),
                        Token::Text(" "),
                        Token::Text("stuff"),
                        Token::Character('\n'),
                        Token::Dedent,
                        Token::Text(" "),
                        Token::Text("closing"),
                        Token::Text(" "),
                        Token::Text("nested"),
                        Token::Dedent,
                        Token::Text("\n")]);
    }
}
