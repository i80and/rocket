use bytecount::naive_count_32;
use regex::Regex;

lazy_static! {
    static ref PAT_TOKENS: Regex = Regex::new(r#"(?xm)
          (?:\(:)
        | (?:=>\n\x20+)
        | (?:\n\x20+)
        | "
        | =
        | \(
        | \)
        | \s+
        | [^\(\)=\s"]+"#).expect("Failed to compile lexer regex");
}

#[derive(Debug, PartialEq)]
pub enum Token<'a> {
    StartBlock(i32),
    RightParen,
    Rocket(i32),
    Dedent,
    Text(i32, &'a str),
    Quote(i32),
}

pub fn lex(data: &str) -> Vec<Token> {
    let data_bytes = data.as_bytes();
    let mut lineno: i32 = 0;
    let mut last_match_start: usize = 0;
    let mut tokens: Vec<Token> = vec![];
    let mut indent: Vec<usize> = vec![0];

    for pat_match in PAT_TOKENS.find_iter(data) {
        lineno += naive_count_32(&data_bytes[last_match_start..pat_match.start()], b'\n') as i32;
        last_match_start = pat_match.start();
        let token_text = pat_match.as_str();
        let bytes = token_text.as_bytes();

        let token = match bytes[0] {
            b')' => Token::RightParen,
            b'"' => Token::Quote(lineno),
            _ if bytes == b"(:" => Token::StartBlock(lineno),
            _ if bytes.starts_with(b"=>\n") => {
                indent.push(bytes.len() - 3);
                Token::Rocket(lineno)
            }
            _ if bytes.starts_with(b"\n") => {
                let mut current_indentation_level =
                    *(indent.last().expect("Indentation stack is empty"));
                let new_indentation_level = token_text.len() - 1;

                while new_indentation_level < current_indentation_level {
                    indent.pop();
                    current_indentation_level =
                        *(indent.last().expect("Indentation stack is empty"));
                    tokens.push(Token::Dedent);
                }

                let new_end = token_text.len() - current_indentation_level;
                Token::Text(lineno, &token_text[..new_end])
            }
            _ => Token::Text(lineno, token_text),
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
        assert_eq!(
            lex(r#"(:foo bar (:a "b c") "baz" )"#),
            vec![
                Token::StartBlock(0),
                Token::Text(0, "foo"),
                Token::Text(0, " "),
                Token::Text(0, "bar"),
                Token::Text(0, " "),
                Token::StartBlock(0),
                Token::Text(0, "a"),
                Token::Text(0, " "),
                Token::Quote(0),
                Token::Text(0, "b"),
                Token::Text(0, " "),
                Token::Text(0, "c"),
                Token::Quote(0),
                Token::RightParen,
                Token::Text(0, " "),
                Token::Quote(0),
                Token::Text(0, "baz"),
                Token::Quote(0),
                Token::Text(0, " "),
                Token::RightParen,
            ]
        );
    }

    #[test]
    fn test_rocket() {
        assert_eq!(
            lex(
                r#"
(:note "a title" =>
  stuff  1

  stuff 2

  (:note =>
    more stuff

    second =>paragraph

  closing nested

(:h2 foo)"#
            ),
            vec![
                Token::Text(0, "\n"),
                Token::StartBlock(1),
                Token::Text(1, "note"),
                Token::Text(1, " "),
                Token::Quote(1),
                Token::Text(1, "a"),
                Token::Text(1, " "),
                Token::Text(1, "title"),
                Token::Quote(1),
                Token::Text(1, " "),
                Token::Rocket(1),
                Token::Text(2, "stuff"),
                Token::Text(2, "  "),
                Token::Text(2, "1"),
                Token::Text(2, "\n\n"),
                Token::Text(4, "stuff"),
                Token::Text(4, " "),
                Token::Text(4, "2"),
                Token::Text(4, "\n\n"),
                Token::StartBlock(6),
                Token::Text(6, "note"),
                Token::Text(6, " "),
                Token::Rocket(6),
                Token::Text(7, "more"),
                Token::Text(7, " "),
                Token::Text(7, "stuff"),
                Token::Text(7, "\n\n"),
                Token::Text(9, "second"),
                Token::Text(9, " "),
                Token::Text(9, "="),
                Token::Text(9, ">paragraph"),
                Token::Dedent,
                Token::Text(9, "\n\n"),
                Token::Text(11, "closing"),
                Token::Text(11, " "),
                Token::Text(11, "nested"),
                Token::Dedent,
                Token::Text(11, "\n\n"),
                Token::StartBlock(13),
                Token::Text(13, "h2"),
                Token::Text(13, " "),
                Token::Text(13, "foo"),
                Token::RightParen,
            ]
        );
    }

    #[test]
    fn test_rocket_indentation() {
        assert_eq!(
            lex(
                r#"
(:note =>
  stuff
    stuff"#.trim()
            ),
            vec![
                Token::StartBlock(0),
                Token::Text(0, "note"),
                Token::Text(0, " "),
                Token::Rocket(0),
                Token::Text(1, "stuff"),
                Token::Text(1, "\n  "),
                Token::Text(2, "stuff"),
                Token::Dedent,
            ]
        );
    }
}
