use comrak::nodes::{TableAlignment, NodeValue, ListType, AstNode};
use comrak;
use typed_arena::Arena;

use highlighter::SyntaxHighlighter;

fn isspace(c: u8) -> bool {
    match c as char {
        '\t' | '\n' | '\x0B' | '\x0C' | '\r' | ' ' => true,
        _ => false,
    }
}

pub struct MarkdownRenderer {
    options: comrak::ComrakOptions,
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        let mut options = comrak::ComrakOptions::default();
        options.github_pre_lang = true;
        options.ext_strikethrough = true;
        options.ext_table = true;

        MarkdownRenderer { options: options }
    }

    pub fn render(&self, markdown: &str, highlighter: &SyntaxHighlighter) -> (String, String) {
        let arena = Arena::new();
        let root = comrak::parse_document(&arena, markdown, &self.options);
        let mut formatter = HtmlFormatter::new(self, highlighter);
        formatter.format(root, false);
        formatter.flush();
        (formatter.s, formatter.title)
    }
}

// Largely purloined from comrak. See COPYING for details.

struct HtmlFormatter<'o> {
    in_title: bool,
    title: String,

    s: String,
    last_level: u32,
    options: &'o comrak::ComrakOptions,
    highlighter: &'o SyntaxHighlighter,
}

impl<'o> HtmlFormatter<'o> {
    fn new(renderer: &'o MarkdownRenderer, highlighter: &'o SyntaxHighlighter) -> Self {
        HtmlFormatter {
            in_title: false,
            title: String::with_capacity(20),

            s: String::with_capacity(1024),
            last_level: 0,
            options: &renderer.options,
            highlighter: highlighter,
        }
    }

    fn cr(&mut self) {
        let l = self.s.len();
        if l > 0 && self.s.as_bytes()[l - 1] != b'\n' {
            self.append_html("\n");
        }
    }

    fn escape(&mut self, buffer: &str) {
        lazy_static! {
            static ref NEEDS_ESCAPED: [bool; 256] = {
                let mut sc = [false; 256];
                for &c in &['"', '&', '<', '>'] {
                    sc[c as usize] = true;
                }
                sc
            };
        }

        let src = buffer.as_bytes();
        let size = src.len();
        let mut i = 0;

        while i < size {
            let org = i;
            while i < size && !NEEDS_ESCAPED[src[i] as usize] {
                i += 1;
            }

            if i > org {
                self.append_html(&buffer[org..i]);
            }

            if i >= size {
                break;
            }

            match src[i] as char {
                '"' => self.append_html("&quot;"),
                '&' => self.append_html("&amp;"),
                '<' => self.append_html("&lt;"),
                '>' => self.append_html("&gt;"),
                _ => unreachable!(),
            }

            i += 1;
        }
    }

    fn escape_href(&mut self, buffer: &str) {
        lazy_static! {
            static ref HREF_SAFE: [bool; 256] = {
                let mut a = [false; 256];
                for &c in concat!("-_.+!*'(),%#@?=;:/,+&$abcdefghijkl",
                "mnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789").as_bytes() {
                    a[c as usize] = true;
                }
                a
            };
        }

        let src = buffer.as_bytes();
        let size = src.len();
        let mut i = 0;

        while i < size {
            let org = i;
            while i < size && HREF_SAFE[src[i] as usize] {
                i += 1;
            }

            if i > org {
                self.s += &buffer[org..i];
            }

            if i >= size {
                break;
            }

            match src[i] as char {
                '&' => self.s += "&amp;",
                '\'' => self.s += "&#x27;",
                _ => self.s += &format!("%{:02X}", src[i]),
            }

            i += 1;
        }
    }

    fn format_children<'a>(&mut self, node: &'a AstNode<'a>, plain: bool) {
        for n in node.children() {
            self.format(n, plain);
        }
    }

    fn format<'a>(&mut self, node: &'a AstNode<'a>, plain: bool) {
        if plain {
            match node.data.borrow().value {
                NodeValue::Text(ref literal) |
                NodeValue::Code(ref literal) |
                NodeValue::HtmlInline(ref literal) => self.escape(literal),
                NodeValue::LineBreak | NodeValue::SoftBreak => self.s.push(' '),
                _ => (),
            }
            self.format_children(node, true);
        } else {
            let new_plain = self.format_node(node, true);
            self.format_children(node, new_plain);
            self.format_node(node, false);
        }
    }

    fn format_node<'a>(&mut self, node: &'a AstNode<'a>, entering: bool) -> bool {
        match node.data.borrow().value {
            NodeValue::Document => (),
            NodeValue::BlockQuote => {
                if entering {
                    self.cr();
                    self.append_html("<blockquote>\n");
                } else {
                    self.cr();
                    self.append_html("</blockquote>\n");
                }
            }
            NodeValue::List(ref nl) => {
                if entering {
                    self.cr();
                    if nl.list_type == ListType::Bullet {
                        self.append_html("<ul>\n");
                    } else if nl.start == 1 {
                        self.append_html("<ol>\n");
                    } else {
                        self.s += &format!("<ol start=\"{}\">\n", nl.start);
                    }
                } else if nl.list_type == ListType::Bullet {
                    self.append_html("</ul>\n");
                } else {
                    self.append_html("</ol>\n");
                }
            }
            NodeValue::Item(..) => {
                if entering {
                    self.cr();
                    self.append_html("<li>");
                } else {
                    self.append_html("</li>\n");
                }
            }
            NodeValue::Heading(ref nch) => {
                let prefix = if nch.level <= self.last_level {
                    "\n</section>".repeat((self.last_level - nch.level + 1) as usize)
                } else {
                    "".to_owned()
                };

                self.last_level = nch.level;

                if entering {
                    self.cr();
                    self.append_html(&format!("{}<section><h{}>", prefix, nch.level));

                    if nch.level == 1 {
                        self.in_title = true;
                    }
                } else {
                    if nch.level == 1 {
                        self.in_title = false;
                    }

                    self.append_html(&format!("</h{}>\n", nch.level));
                }
            }
            NodeValue::CodeBlock(ref ncb) => {
                if entering {
                    self.cr();

                    if ncb.info.is_empty() {
                        self.append_html("<pre><code>");
                        self.escape(&ncb.literal);
                    } else {
                        let mut first_tag = 0;
                        while first_tag < ncb.info.len() &&
                              !isspace(ncb.info.as_bytes()[first_tag]) {
                            first_tag += 1;
                        }

                        self.append_html("<pre lang=\"");
                        self.escape(&ncb.info[..first_tag]);
                        self.append_html("\"><code>");

                        match self.highlighter
                                  .highlight(&ncb.info[..first_tag], &ncb.literal) {
                            Ok(s) => {
                                self.s += &s;
                            }
                            Err(_) => {
                                self.escape(&ncb.literal);
                            }
                        }
                    }

                    self.append_html("</code></pre>\n");
                }
            }
            NodeValue::HtmlBlock(ref nhb) => {
                if entering {
                    self.cr();
                    self.s += &nhb.literal;
                    self.cr();
                }
            }
            NodeValue::ThematicBreak => {
                if entering {
                    self.cr();
                    self.append_html("<hr />\n");
                }
            }
            NodeValue::Paragraph => {
                let tight = match node.parent()
                          .and_then(|n| n.parent())
                          .map(|n| n.data.borrow().value.clone()) {
                    Some(NodeValue::List(nl)) => nl.tight,
                    _ => false,
                };

                if entering {
                    if !tight {
                        self.cr();
                        self.append_html("<p>");
                    }
                } else if !tight {
                    self.append_html("</p>\n");
                }
            }
            NodeValue::Text(ref literal) => {
                if entering {
                    self.escape(literal);
                }
            }
            NodeValue::LineBreak => {
                if entering {
                    self.append_html("<br />\n");
                }
            }
            NodeValue::SoftBreak => {
                if entering {
                    if self.options.hardbreaks {
                        self.append_html("<br />\n");
                    } else {
                        self.append_html("\n");
                    }
                }
            }
            NodeValue::Code(ref literal) => {
                if entering {
                    self.append_html("<code>");
                    self.escape(literal);
                    self.append_html("</code>");
                }
            }
            NodeValue::HtmlInline(ref literal) => {
                if entering {
                    self.append_html(literal);
                }
            }
            NodeValue::Strong => {
                if entering {
                    self.append_html("<strong>");
                } else {
                    self.append_html("</strong>");
                }
            }
            NodeValue::Emph => {
                if entering {
                    self.append_html("<em>");
                } else {
                    self.append_html("</em>");
                }
            }
            NodeValue::Strikethrough => {
                if entering {
                    self.append_html("<del>");
                } else {
                    self.append_html("</del>");
                }
            }
            NodeValue::Superscript => {
                if entering {
                    self.append_html("<sup>");
                } else {
                    self.append_html("</sup>");
                }
            }
            NodeValue::Link(ref nl) => {
                if entering {
                    self.append_html("<a href=\"");
                    self.escape_href(&nl.url);
                    if !nl.title.is_empty() {
                        self.append_html("\" title=\"");
                        self.escape(&nl.title);
                    }
                    self.append_html("\">");
                } else {
                    self.append_html("</a>");
                }
            }
            NodeValue::Image(ref nl) => {
                if entering {
                    self.append_html("<img src=\"");
                    self.escape_href(&nl.url);
                    self.append_html("\" alt=\"");
                    return true;
                } else {
                    if !nl.title.is_empty() {
                        self.append_html("\" title=\"");
                        self.escape(&nl.title);
                    }
                    self.append_html("\" />");
                }
            }
            NodeValue::Table(..) => {
                if entering {
                    self.cr();
                    self.append_html("<table>\n");
                } else {
                    if !node.last_child()
                            .unwrap()
                            .same_node(node.first_child().unwrap()) {
                        self.append_html("</tbody>");
                    }
                    self.append_html("</table>\n");
                }
            }
            NodeValue::TableRow(header) => {
                if entering {
                    self.cr();
                    if header {
                        self.append_html("<thead>");
                        self.cr();
                    }
                    self.append_html("<tr>");
                } else {
                    self.cr();
                    self.append_html("</tr>");
                    if header {
                        self.cr();
                        self.append_html("</thead>");
                        self.cr();
                        self.append_html("<tbody>");
                    }
                }
            }
            NodeValue::TableCell => {
                let row = &node.parent().unwrap().data.borrow().value;
                let in_header = match *row {
                    NodeValue::TableRow(header) => header,
                    _ => panic!(),
                };

                let table = &node.parent().unwrap().parent().unwrap().data.borrow().value;
                let alignments = match *table {
                    NodeValue::Table(ref alignments) => alignments,
                    _ => panic!(),
                };

                if entering {
                    self.cr();
                    if in_header {
                        self.append_html("<th");
                    } else {
                        self.append_html("<td");
                    }

                    let mut start = node.parent().unwrap().first_child().unwrap();
                    let mut i = 0;
                    while !start.same_node(node) {
                        i += 1;
                        start = start.next_sibling().unwrap();
                    }

                    match alignments[i] {
                        TableAlignment::Left => self.append_html(" align=\"left\""),
                        TableAlignment::Right => self.append_html(" align=\"right\""),
                        TableAlignment::Center => self.append_html(" align=\"center\""),
                        TableAlignment::None => (),
                    }

                    self.append_html(">");
                } else if in_header {
                    self.append_html("</th>");
                } else {
                    self.append_html("</td>");
                }
            }
        }
        false
    }

    /// Append text that should not appear in a plain text context.
    fn append_html(&mut self, text: &str) {
        if self.in_title {
            self.title += text;
        }

        self.s += text;
    }

    fn flush(&mut self) {
        let ending_tags = "</section>".repeat(self.last_level as usize);
        self.append_html(&ending_tags);
        self.last_level = 0;
    }
}
