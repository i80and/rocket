use std::collections::HashSet;
use regex::Regex;

lazy_static! {
    static ref PAT_PARAGRAPHS: Regex = Regex::new(r#"(?xm)
          (?:\n\n) | (?:</?[a-z0-9]+) | (?:[^\n<]+) | \n"#).expect("Failed to compile paragraph regex");
    static ref BLOCK_TAGS: HashSet<&'static [u8]> = vec![
         b"address".as_ref(),
         b"article".as_ref(),
         b"aside".as_ref(),
         b"blockquote".as_ref(),
         b"details".as_ref(),
         b"div".as_ref(),
         b"dl".as_ref(),
         b"fieldset".as_ref(),
         b"figcaption".as_ref(),
         b"figure".as_ref(),
         b"footer".as_ref(),
         b"form".as_ref(),
         b"h1".as_ref(),
         b"h2".as_ref(),
         b"h3".as_ref(),
         b"h4".as_ref(),
         b"h5".as_ref(),
         b"h6".as_ref(),
         b"header".as_ref(),
         b"hgroup".as_ref(),
         b"hr".as_ref(),
         b"main".as_ref(),
         b"menu".as_ref(),
         b"nav".as_ref(),
         b"ol".as_ref(),
         b"p".as_ref(),
         b"pre".as_ref(),
         b"section".as_ref(),
         b"table".as_ref(),
         b"ul".as_ref(),
    ].into_iter().collect();
}

pub fn inject_paragraphs(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 14);
    let mut pre: i32 = 0;

    for pat_match in PAT_PARAGRAPHS.find_iter(text) {
        let match_str = pat_match.as_str();
        let match_bytes = match_str.as_bytes();
        if match_str.starts_with("\n\n") {
            if pre == 0 {
                if result.ends_with("<p>") {
                    let len = result.len();
                    result.truncate(len - 3);
                }

                result.push_str("\n\n<p>");
            } else {
                result.push_str(match_str);
            }

            continue;
        }

        if match_bytes.get(0) == Some(&b'<') {
            let (tag_name, closing) = match match_bytes.get(1) {
                Some(&b'/') => (&match_bytes[2..], true),
                _ => (&match_bytes[1..], false),
            };

            let is_block = BLOCK_TAGS.contains(tag_name);

            if tag_name == b"pre" {
                if !closing && pre == 0 {
                    pre += 1;
                } else if closing && pre > 0 {
                    pre -= 1;
                }
            }

            if is_block && result.ends_with("<p>") {
                let len = result.len();
                result.truncate(len - 3);
            }
        }

        result.push_str(match_str);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_paragraphs() {
        assert_eq!(inject_paragraphs(""), "".to_owned());

        let src = r#"<section><h1 id="home">Home</h1>

Rocket is a fast, powerful, and homoiconic text markup format.

Example ref: <a href="tutorials/writing-your-first-project">Writing Your First Project</a>.

<section><h2 id="level-2-title">Level 2 Title</h2>

<h2 id="same-level">Same Level</h2>

<section><h3 id="level-3-title">Level 3 Title</h3>

</section><h2 id="back-up-to-level-2">Back Up to Level 2</h2>

<div class="steps"><div class="steps__step"><div class="steps__bullet"><div class="steps__stepnumber">3</div></div><h4>Third Step</h4><div>Lorem ipsum

Sed facilisis
</div></div></div>

<pre style="background-color:#eff1f5;">
<span style="color:#b48ead;">sudo</span>

<span style="color:#b48ead;">clear</span>

</pre>

</section></section>"#;

        let expected = r#"<section><h1 id="home">Home</h1>

<p>Rocket is a fast, powerful, and homoiconic text markup format.

<p>Example ref: <a href="tutorials/writing-your-first-project">Writing Your First Project</a>.

<section><h2 id="level-2-title">Level 2 Title</h2>

<h2 id="same-level">Same Level</h2>

<section><h3 id="level-3-title">Level 3 Title</h3>

</section><h2 id="back-up-to-level-2">Back Up to Level 2</h2>

<div class="steps"><div class="steps__step"><div class="steps__bullet"><div class="steps__stepnumber">3</div></div><h4>Third Step</h4><div>Lorem ipsum

<p>Sed facilisis
</div></div></div>

<pre style="background-color:#eff1f5;">
<span style="color:#b48ead;">sudo</span>

<span style="color:#b48ead;">clear</span>

</pre>

</section></section>"#;

        assert_eq!(inject_paragraphs(src), expected.to_owned());
    }
}
