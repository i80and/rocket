use comrak::nodes::{TableAlignment, NodeValue, ListType, AstNode};
use comrak;
use syntect;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use lazycell::LazyCell;
use typed_arena::Arena;

fn isspace(c: u8) -> bool {
    match c as char {
        '\t' | '\n' | '\x0B' | '\x0C' | '\r' | ' ' => true,
        _ => false,
    }
}

pub struct MarkdownRenderer {
    options: comrak::ComrakOptions,
    syntax_set: LazyCell<SyntaxSet>,
    theme_set: LazyCell<ThemeSet>,
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        let mut options = comrak::ComrakOptions::default();
        options.github_pre_lang = true;
        options.ext_strikethrough = true;
        options.ext_table = true;

        MarkdownRenderer {
            options: options,
            syntax_set: LazyCell::new(),
            theme_set: LazyCell::new(),
        }
    }

    pub fn render(&self, markdown: &str) -> String {
        let arena = Arena::new();
        let root = comrak::parse_document(&arena, markdown, &self.options);
        let mut formatter = HtmlFormatter::new(self);
        formatter.format(root, false);
        formatter.flush();
        formatter.s
    }
}

// Largely purloined  from comrak. See COPYING for details.

struct HtmlFormatter<'o> {
    s: String,
    last_level: u32,
    options: &'o comrak::ComrakOptions,
    syntax_set: &'o SyntaxSet,
    theme_set: &'o ThemeSet,
}

impl<'o> HtmlFormatter<'o> {
    fn new(renderer: &'o MarkdownRenderer) -> Self {
        HtmlFormatter {
            s: String::with_capacity(1024),
            last_level: 0,
            options: &renderer.options,
            syntax_set: renderer.syntax_set.borrow_with(|| SyntaxSet::load_defaults_newlines()),
            theme_set: renderer.theme_set.borrow_with(|| ThemeSet::load_defaults()),
        }
    }

    fn cr(&mut self) {
        let l = self.s.len();
        if l > 0 && self.s.as_bytes()[l - 1] != b'\n' {
            self.s += "\n";
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
                self.s += &buffer[org..i];
            }

            if i >= size {
                break;
            }

            match src[i] as char {
                '"' => self.s += "&quot;",
                '&' => self.s += "&amp;",
                '<' => self.s += "&lt;",
                '>' => self.s += "&gt;",
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
                    self.s += "<blockquote>\n";
                } else {
                    self.cr();
                    self.s += "</blockquote>\n";
                }
            }
            NodeValue::List(ref nl) => {
                if entering {
                    self.cr();
                    if nl.list_type == ListType::Bullet {
                        self.s += "<ul>\n";
                    } else if nl.start == 1 {
                        self.s += "<ol>\n";
                    } else {
                        self.s += &format!("<ol start=\"{}\">\n", nl.start);
                    }
                } else if nl.list_type == ListType::Bullet {
                    self.s += "</ul>\n";
                } else {
                    self.s += "</ol>\n";
                }
            }
            NodeValue::Item(..) => {
                if entering {
                    self.cr();
                    self.s += "<li>";
                } else {
                    self.s += "</li>\n";
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
                    self.s += &format!("{}<section><h{}>", prefix, nch.level);
                } else {
                    self.s += &format!("</h{}>\n", nch.level);
                }
            }
            NodeValue::CodeBlock(ref ncb) => {
                if entering {
                    self.cr();

                    if ncb.info.is_empty() {
                        self.s += "<pre><code>";
                        self.escape(&ncb.literal);
                    } else {
                        let mut first_tag = 0;
                        while first_tag < ncb.info.len() &&
                              !isspace(ncb.info.as_bytes()[first_tag]) {
                            first_tag += 1;
                        }

                        self.s += "<pre lang=\"";
                        self.escape(&ncb.info[..first_tag]);
                        self.s += "\"><code>";

                        match self.highlight(&ncb.info[..first_tag], &ncb.literal) {
                            Ok(s) => { self.s += &s; },
                            Err(_) => { self.escape(&ncb.literal); },
                        }
                    }

                    self.s += "</code></pre>\n";
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
                    self.s += "<hr />\n";
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
                        self.s += "<p>";
                    }
                } else if !tight {
                    self.s += "</p>\n";
                }
            }
            NodeValue::Text(ref literal) => {
                if entering {
                    self.escape(literal);
                }
            }
            NodeValue::LineBreak => {
                if entering {
                    self.s += "<br />\n";
                }
            }
            NodeValue::SoftBreak => {
                if entering {
                    if self.options.hardbreaks {
                        self.s += "<br />\n";
                    } else {
                        self.s += "\n";
                    }
                }
            }
            NodeValue::Code(ref literal) => {
                if entering {
                    self.s += "<code>";
                    self.escape(literal);
                    self.s += "</code>";
                }
            }
            NodeValue::HtmlInline(ref literal) => {
                if entering {
                    self.s += literal;
                }
            }
            NodeValue::Strong => {
                if entering {
                    self.s += "<strong>";
                } else {
                    self.s += "</strong>";
                }
            }
            NodeValue::Emph => {
                if entering {
                    self.s += "<em>";
                } else {
                    self.s += "</em>";
                }
            }
            NodeValue::Strikethrough => {
                if entering {
                    self.s += "<del>";
                } else {
                    self.s += "</del>";
                }
            }
            NodeValue::Superscript => {
                if entering {
                    self.s += "<sup>";
                } else {
                    self.s += "</sup>";
                }
            }
            NodeValue::Link(ref nl) => {
                if entering {
                    self.s += "<a href=\"";
                    self.escape_href(&nl.url);
                    if !nl.title.is_empty() {
                        self.s += "\" title=\"";
                        self.escape(&nl.title);
                    }
                    self.s += "\">";
                } else {
                    self.s += "</a>";
                }
            }
            NodeValue::Image(ref nl) => {
                if entering {
                    self.s += "<img src=\"";
                    self.escape_href(&nl.url);
                    self.s += "\" alt=\"";
                    return true;
                } else {
                    if !nl.title.is_empty() {
                        self.s += "\" title=\"";
                        self.escape(&nl.title);
                    }
                    self.s += "\" />";
                }
            }
            NodeValue::Table(..) => {
                if entering {
                    self.cr();
                    self.s += "<table>\n";
                } else {
                    if !node.last_child()
                            .unwrap()
                            .same_node(node.first_child().unwrap()) {
                        self.s += "</tbody>";
                    }
                    self.s += "</table>\n";
                }
            }
            NodeValue::TableRow(header) => {
                if entering {
                    self.cr();
                    if header {
                        self.s += "<thead>";
                        self.cr();
                    }
                    self.s += "<tr>";
                } else {
                    self.cr();
                    self.s += "</tr>";
                    if header {
                        self.cr();
                        self.s += "</thead>";
                        self.cr();
                        self.s += "<tbody>";
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
                        self.s += "<th";
                    } else {
                        self.s += "<td";
                    }

                    let mut start = node.parent().unwrap().first_child().unwrap();
                    let mut i = 0;
                    while !start.same_node(node) {
                        i += 1;
                        start = start.next_sibling().unwrap();
                    }

                    match alignments[i] {
                        TableAlignment::Left => self.s += " align=\"left\"",
                        TableAlignment::Right => self.s += " align=\"right\"",
                        TableAlignment::Center => self.s += " align=\"center\"",
                        TableAlignment::None => (),
                    }

                    self.s += ">";
                } else if in_header {
                    self.s += "</th>";
                } else {
                    self.s += "</td>";
                }
            }
        }
        false
    }

    fn highlight(&self, language: &str, code: &str) -> Result<String, ()> {
        let syntax = self.syntax_set.find_syntax_by_extension(language).ok_or(())?;
        let theme = &self.theme_set.themes["base16-ocean.dark"];

        Ok(syntect::html::highlighted_snippet_for_string(&code, &syntax, theme))
    }

    fn flush(&mut self) {
        self.s += &"</section>".repeat(self.last_level as usize);
        self.last_level = 0;
    }
}
