use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashSet;
use std::io::{self, Write};
use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};
use comrak;
use regex::Regex;
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
        let mut vec = vec![];
        let root = comrak::parse_document(&arena, markdown, &self.options);
        let title = {
            let mut writer = WriteWithLast {
                output: &mut vec,
                last_was_lf: Cell::new(true),
            };
            let mut formatter = HtmlFormatter::new(self, highlighter, &mut writer);
            formatter
                .format(root, false)
                .expect("Failed to format markdown");
            formatter.flush();
            formatter.title
        };
        (String::from_utf8_lossy(&vec).into_owned(), title)
    }
}

// Largely purloined from comrak. See COPYING for details.

pub struct WriteWithLast<'w> {
    output: &'w mut Write,
    pub last_was_lf: Cell<bool>,
}

impl<'w> Write for WriteWithLast<'w> {
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let l = buf.len();
        if l > 0 {
            self.last_was_lf.set(buf[l - 1] == 10);
        }
        self.output.write(buf)
    }
}


struct HtmlFormatter<'o> {
    in_title: bool,
    title: String,
    highlighter: &'o SyntaxHighlighter,
    last_level: u32,

    output: &'o mut WriteWithLast<'o>,
    options: &'o comrak::ComrakOptions,
    seen_anchors: HashSet<String>,
}

const NEEDS_ESCAPED: [bool; 256] = [
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    true,
    false,
    false,
    false,
    true,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    true,
    false,
    true,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
];

fn tagfilter(literal: &[u8]) -> bool {
    lazy_static! {
        static ref TAGFILTER_BLACKLIST: [&'static str; 9] =
            ["title", "textarea", "style", "xmp", "iframe",
             "noembed", "noframes", "script", "plaintext"];
    }

    if literal.len() < 3 || literal[0] != b'<' {
        return false;
    }

    let mut i = 1;
    if literal[i] == b'/' {
        i += 1;
    }

    for t in TAGFILTER_BLACKLIST.iter() {
        if unsafe { String::from_utf8_unchecked(literal[i..].to_vec()) }
            .to_lowercase()
            .starts_with(t)
        {
            let j = i + t.len();
            return isspace(literal[j]) || literal[j] == b'>'
                || (literal[j] == b'/' && literal.len() >= j + 2 && literal[j + 1] == b'>');
        }
    }

    false
}

fn tagfilter_block(input: &[u8], o: &mut Write) -> io::Result<()> {
    let size = input.len();
    let mut i = 0;

    while i < size {
        let org = i;
        while i < size && input[i] != b'<' {
            i += 1;
        }

        if i > org {
            try!(o.write_all(&input[org..i]));
        }

        if i >= size {
            break;
        }

        if tagfilter(&input[i..]) {
            try!(o.write_all(b"&lt;"));
        } else {
            try!(o.write_all(b"<"));
        }

        i += 1;
    }

    Ok(())
}

impl<'o> HtmlFormatter<'o> {
    fn new(
        renderer: &'o MarkdownRenderer,
        highlighter: &'o SyntaxHighlighter,
        output: &'o mut WriteWithLast<'o>,
    ) -> Self {
        HtmlFormatter {
            in_title: false,
            title: String::with_capacity(20),
            highlighter: highlighter,
            last_level: 0,

            options: &renderer.options,
            output: output,
            seen_anchors: HashSet::new(),
        }
    }

    fn cr(&mut self) -> io::Result<()> {
        if !self.output.last_was_lf.get() {
            try!(self.append_html(b"\n"));
        }
        Ok(())
    }

    fn escape(&mut self, buffer: &[u8]) -> io::Result<()> {
        let size = buffer.len();
        let mut i = 0;

        while i < size {
            let org = i;
            while i < size && !NEEDS_ESCAPED[buffer[i] as usize] {
                i += 1;
            }

            if i > org {
                try!(self.append_html(&buffer[org..i]));
            }

            if i >= size {
                break;
            }

            match buffer[i] as char {
                '"' => {
                    try!(self.append_html(b"&quot;"));
                }
                '&' => {
                    try!(self.append_html(b"&amp;"));
                }
                '<' => {
                    try!(self.append_html(b"&lt;"));
                }
                '>' => {
                    try!(self.append_html(b"&gt;"));
                }
                _ => unreachable!(),
            }

            i += 1;
        }

        Ok(())
    }

    fn escape_href(&mut self, buffer: &[u8]) -> io::Result<()> {
        lazy_static! {
            static ref HREF_SAFE: [bool; 256] = {
                let mut a = [false; 256];
                for &c in b"-_.+!*'(),%#@?=;:/,+&$abcdefghijklmnopqrstuvwxyz".iter() {
                    a[c as usize] = true;
                }
                for &c in b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".iter() {
                    a[c as usize] = true;
                }
                a
            };
        }

        let size = buffer.len();
        let mut i = 0;

        while i < size {
            let org = i;
            while i < size && HREF_SAFE[buffer[i] as usize] {
                i += 1;
            }

            if i > org {
                try!(self.append_html(&buffer[org..i]));
            }

            if i >= size {
                break;
            }

            match buffer[i] as char {
                '&' => {
                    try!(self.append_html(b"&amp;"));
                }
                '\'' => {
                    try!(self.append_html(b"&#x27;"));
                }
                _ => try!(write!(self.output, "%{:02X}", buffer[i])),
            }

            i += 1;
        }

        Ok(())
    }

    fn format_children<'a>(&mut self, node: &'a AstNode<'a>, plain: bool) -> io::Result<()> {
        for n in node.children() {
            try!(self.format(n, plain));
        }
        Ok(())
    }

    fn format<'a>(&mut self, node: &'a AstNode<'a>, plain: bool) -> io::Result<()> {
        if plain {
            match node.data.borrow().value {
                NodeValue::Text(ref literal) |
                NodeValue::Code(ref literal) |
                NodeValue::HtmlInline(ref literal) => {
                    try!(self.escape(literal));
                }
                NodeValue::LineBreak | NodeValue::SoftBreak => {
                    try!(self.append_html(b" "));
                }
                _ => (),
            }
            try!(self.format_children(node, true));
        } else {
            let new_plain = try!(self.format_node(node, true));
            try!(self.format_children(node, new_plain));
            try!(self.format_node(node, false));
        }

        Ok(())
    }

    fn collect_text<'a>(&self, node: &'a AstNode<'a>, output: &mut Vec<u8>) {
        match node.data.borrow().value {
            NodeValue::Text(ref literal) | NodeValue::Code(ref literal) => {
                output.extend_from_slice(literal)
            }
            NodeValue::LineBreak | NodeValue::SoftBreak => output.push(b' '),
            _ => for n in node.children() {
                self.collect_text(n, output);
            },
        }
    }

    fn format_node<'a>(&mut self, node: &'a AstNode<'a>, entering: bool) -> io::Result<bool> {
        match node.data.borrow().value {
            NodeValue::Document => (),
            NodeValue::BlockQuote => if entering {
                try!(self.cr());
                try!(self.append_html(b"<blockquote>\n"));
            } else {
                try!(self.cr());
                try!(self.append_html(b"</blockquote>\n"));
            },
            NodeValue::List(ref nl) => if entering {
                try!(self.cr());
                if nl.list_type == ListType::Bullet {
                    try!(self.append_html(b"<ul>\n"));
                } else if nl.start == 1 {
                    try!(self.append_html(b"<ol>\n"));
                } else {
                    try!(write!(self.output, "<ol start=\"{}\">\n", nl.start));
                }
            } else if nl.list_type == ListType::Bullet {
                try!(self.append_html(b"</ul>\n"));
            } else {
                try!(self.append_html(b"</ol>\n"));
            },
            NodeValue::Item(..) => if entering {
                try!(self.cr());
                try!(self.append_html(b"<li>"));
            } else {
                try!(self.append_html(b"</li>\n"));
            },
            NodeValue::Heading(ref nch) => {
                lazy_static! {
                    static ref REJECTED_CHARS: Regex = Regex::new(r"[^\p{L}\p{M}\p{N}\p{Pc} -]").unwrap();
                }

                let prefix = if nch.level <= self.last_level {
                    "\n</section>".repeat((self.last_level - nch.level + 1) as usize)
                } else {
                    "".to_owned()
                };

                self.last_level = nch.level;

                if entering {
                    if nch.level == 1 {
                        self.in_title = true;
                    }

                    try!(self.cr());
                    try!(write!(self.output, "{}<section><h{}>", prefix, nch.level));

                    if let Some(ref prefix) = self.options.ext_header_ids {
                        let mut text_content = Vec::with_capacity(20);
                        self.collect_text(node, &mut text_content);

                        let mut id = String::from_utf8(text_content).unwrap();
                        id = id.to_lowercase();
                        id = REJECTED_CHARS.replace(&id, "").to_string();
                        id = id.replace(' ', "-");

                        let mut uniq = 0;
                        id = loop {
                            let anchor = if uniq == 0 {
                                Cow::from(&*id)
                            } else {
                                Cow::from(format!("{}-{}", &id, uniq))
                            };

                            if !self.seen_anchors.contains(&*anchor) {
                                break anchor.to_string();
                            }

                            uniq += 1;
                        };

                        self.seen_anchors.insert(id.clone());

                        try!(write!(
                            self.output,
                            "<a href=\"#{}\" aria-hidden=\"true\" class=\"anchor\" id=\"{}{}\"></a>",
                            id,
                            prefix,
                            id
                        ));
                    }
                } else {
                    if nch.level == 1 {
                        self.in_title = false;
                    }

                    try!(write!(self.output, "</h{}>\n", nch.level));
                }
            }
            NodeValue::CodeBlock(ref ncb) => if entering {
                try!(self.cr());

                if ncb.info.is_empty() {
                    try!(self.append_html(b"<pre><code>"));
                } else {
                    let mut first_tag = 0;
                    while first_tag < ncb.info.len() && !isspace(ncb.info[first_tag]) {
                        first_tag += 1;
                    }

                    try!(self.append_html(b"<pre lang=\""));

                    let tag = ncb.info[..first_tag].to_owned();
                    let tag = String::from_utf8(tag)
                        .ok()
                        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, ""))?;
                    let literal = ncb.literal.to_owned();
                    let literal = String::from_utf8(literal)
                        .ok()
                        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, ""))?;

                    match self.highlighter.highlight(&tag, &literal) {
                        Ok(s) => {
                            try!(self.append_html(s.as_bytes()));
                        }
                        Err(_) => {
                            try!(self.escape(&ncb.info[..first_tag]));
                        }
                    }
                    try!(self.escape(&ncb.info[..first_tag]));
                    try!(self.append_html(b"\"><code>"));
                }
                try!(self.escape(&ncb.literal));
                try!(self.append_html(b"</code></pre>\n"));
            },
            NodeValue::HtmlBlock(ref nhb) => if entering {
                try!(self.cr());
                if self.options.ext_tagfilter {
                    try!(tagfilter_block(&nhb.literal, &mut self.output));
                } else {
                    try!(self.append_html(&nhb.literal));
                }
                try!(self.cr());
            },
            NodeValue::ThematicBreak => if entering {
                try!(self.cr());
                try!(self.append_html(b"<hr />\n"));
            },
            NodeValue::Paragraph => {
                let tight = match node.parent()
                    .and_then(|n| n.parent())
                    .map(|n| n.data.borrow().value.clone())
                {
                    Some(NodeValue::List(nl)) => nl.tight,
                    _ => false,
                };

                if entering {
                    if !tight {
                        try!(self.cr());
                        try!(self.append_html(b"<p>"));
                    }
                } else if !tight {
                    try!(self.append_html(b"</p>\n"));
                }
            }
            NodeValue::Text(ref literal) => if entering {
                try!(self.escape(literal));
            },
            NodeValue::LineBreak => if entering {
                try!(self.append_html(b"<br />\n"));
            },
            NodeValue::SoftBreak => if entering {
                if self.options.hardbreaks {
                    try!(self.append_html(b"<br />\n"));
                } else {
                    try!(self.append_html(b"\n"));
                }
            },
            NodeValue::Code(ref literal) => if entering {
                try!(self.append_html(b"<code>"));
                try!(self.escape(literal));
                try!(self.append_html(b"</code>"));
            },
            NodeValue::HtmlInline(ref literal) => if entering {
                if self.options.ext_tagfilter && tagfilter(literal) {
                    try!(self.append_html(b"&lt;"));
                    try!(self.append_html(&literal[1..]));
                } else {
                    try!(self.append_html(literal));
                }
            },
            NodeValue::Strong => if entering {
                try!(self.append_html(b"<strong>"));
            } else {
                try!(self.append_html(b"</strong>"));
            },
            NodeValue::Emph => if entering {
                try!(self.append_html(b"<em>"));
            } else {
                try!(self.append_html(b"</em>"));
            },
            NodeValue::Strikethrough => if entering {
                try!(self.append_html(b"<del>"));
            } else {
                try!(self.append_html(b"</del>"));
            },
            NodeValue::Superscript => if entering {
                try!(self.append_html(b"<sup>"));
            } else {
                try!(self.append_html(b"</sup>"));
            },
            NodeValue::Link(ref nl) => if entering {
                try!(self.append_html(b"<a href=\""));
                try!(self.escape_href(&nl.url));
                if !nl.title.is_empty() {
                    try!(self.append_html(b"\" title=\""));
                    try!(self.escape(&nl.title));
                }
                try!(self.append_html(b"\">"));
            } else {
                try!(self.append_html(b"</a>"));
            },
            NodeValue::Image(ref nl) => if entering {
                try!(self.append_html(b"<img src=\""));
                try!(self.escape_href(&nl.url));
                try!(self.append_html(b"\" alt=\""));
                return Ok(true);
            } else {
                if !nl.title.is_empty() {
                    try!(self.append_html(b"\" title=\""));
                    try!(self.escape(&nl.title));
                }
                try!(self.append_html(b"\" />"));
            },
            NodeValue::Table(..) => if entering {
                try!(self.cr());
                try!(self.append_html(b"<table>\n"));
            } else {
                if !node.last_child()
                    .unwrap()
                    .same_node(node.first_child().unwrap())
                {
                    try!(self.append_html(b"</tbody>"));
                }
                try!(self.append_html(b"</table>\n"));
            },
            NodeValue::TableRow(header) => if entering {
                try!(self.cr());
                if header {
                    try!(self.append_html(b"<thead>"));
                    try!(self.cr());
                }
                try!(self.append_html(b"<tr>"));
            } else {
                try!(self.cr());
                try!(self.append_html(b"</tr>"));
                if header {
                    try!(self.cr());
                    try!(self.append_html(b"</thead>"));
                    try!(self.cr());
                    try!(self.append_html(b"<tbody>"));
                }
            },
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
                    try!(self.cr());
                    if in_header {
                        try!(self.append_html(b"<th"));
                    } else {
                        try!(self.append_html(b"<td"));
                    }

                    let mut start = node.parent().unwrap().first_child().unwrap();
                    let mut i = 0;
                    while !start.same_node(node) {
                        i += 1;
                        start = start.next_sibling().unwrap();
                    }

                    match alignments[i] {
                        TableAlignment::Left => {
                            try!(self.append_html(b" align=\"left\""));
                        }
                        TableAlignment::Right => {
                            try!(self.append_html(b" align=\"right\""));
                        }
                        TableAlignment::Center => {
                            try!(self.append_html(b" align=\"center\""));
                        }
                        TableAlignment::None => (),
                    }

                    try!(self.append_html(b">"));
                } else if in_header {
                    try!(self.append_html(b"</th>"));
                } else {
                    try!(self.append_html(b"</td>"));
                }
            }
        }
        Ok(false)
    }

    /// Append text that should not appear in a plain text context.
    fn append_html(&mut self, text: &[u8]) -> Result<(), io::Error> {
        if self.in_title {
            self.title += String::from_utf8_lossy(text).as_ref();
        }

        self.output.write_all(text)
    }

    fn flush(&mut self) {
        let ending_tags = "</section>".repeat(self.last_level as usize);
        self.append_html(ending_tags.as_bytes())
            .expect("Failed to flush markdown formatter");
        self.last_level = 0;
    }
}
