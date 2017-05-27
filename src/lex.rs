use regex::Regex;

lazy_static! {
    static ref PAT_EMPTY_LINE: Regex = Regex::new(r#"^\s*\n$"#).expect("Failed to compile whitespace regex");
    static ref PAT_TOKENS: Regex = Regex::new(r#"(?x)
          (?:\(:)
        | (?:=>)
        | "
            (?:
                  [^"\\]
                | \\(.|\n)
                | \\u[0-9a-fA-F]{4}
            )*
          "
        | (?:\n\x20*)
        | =
        | \(
        | \)
        | \s+
        | [^\(\)=\s]+"#).expect("Failed to compile lexer regex");
}

#[derive(Debug)]
pub enum Token<'a> {
    StartBlock,
    RightParen,
    Rocket,
    Indent,
    Dedent,
    Text(&'a str),
    Character(char),
    String(&'a str)
}

pub fn lex<'a>(data: &'a str) -> Vec<Token<'a>> {
    let mut tokens: Vec<Token> = vec![];
    let mut indent: Vec<usize> = vec![0];
    let mut start_rocket = false;

    for pat_match in PAT_TOKENS.find_iter(data) {
        let token_text = pat_match.as_str();
        let bytes = token_text.as_bytes();
        let token = match bytes[0] {
            b')' => Token::RightParen,
            b'"' => Token::String(&token_text[1..token_text.len()-1]),
            b'\n' => {
                if PAT_EMPTY_LINE.is_match(token_text) {
                    tokens.push(Token::Character('\n'));
                    continue;
                }

                let new_indentation_level = token_text.len();
                let current_indentation_level = *(indent.last().expect("Indentation stack is empty"));
                if new_indentation_level < current_indentation_level {
                    indent.pop();
                    tokens.push(Token::Dedent);
                }

                if start_rocket {
                    indent.push(new_indentation_level);
                    tokens.push(Token::Indent);
                    start_rocket = false;
                    Token::Text(&token_text[new_indentation_level..])
                } else {
                    println!("{} {:?}", current_indentation_level, token_text);
                    Token::Text(&token_text[current_indentation_level..])
                }
            },
            b'(' => match bytes.get(1) {
                Some(&b':') => Token::StartBlock,
                None => Token::Character('('),
                _ => panic!("Bad character matched: Expected ':' or nothing")
            },
            b'=' => match bytes.get(1) {
                Some(&b'>') => {
                    start_rocket = true;
                    Token::Rocket
                },
                None => Token::Character('='),
                _ => panic!("Bad character matched: Expected '>' or nothing")
            },
            _ => Token::Text(token_text)
        };

        println!("{:?}", token);
        tokens.push(token)
    }

    while indent.len() > 1 {
        tokens.push(Token::Dedent);
        indent.pop();
    }

    return tokens;
}
