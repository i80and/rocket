extern crate comrak;
#[macro_use] extern crate lazy_static;
extern crate regex;
extern crate time;

mod lex;
mod parse;

use std::env;
use std::collections::HashMap;
use parse::{parse, Node};

struct Evaluator {
    directives: HashMap<String, ()>,
}

impl Evaluator {
    fn new() -> Evaluator {
        Evaluator {
            directives: HashMap::new(),
        }
    }

    fn register<S: Into<String>>(&mut self, name: S) {
        self.directives.insert(name.into(), ());
    }

    fn evaluate(&mut self, node: &Node) -> String {
        "".to_owned()
    }

    fn render_markdown(&self, markdown: &str) -> String {
        let mut options = comrak::ComrakOptions::default();
        options.github_pre_lang = true;
        options.ext_strikethrough = true;

        return comrak::markdown_to_html(&markdown, &options);
    }
}

fn main() {
    let mut evaluator = Evaluator::new();
    evaluator.register("table");
    evaluator.register("version");
    evaluator.register("note");
    evaluator.register("warning");
    evaluator.register("insert");
    evaluator.register("definition-list");
    evaluator.register("manual");

    evaluator.register("concat");
    evaluator.register("join");
    evaluator.register("b");
    evaluator.register("em");
    evaluator.register("s");
    evaluator.register("super");
    evaluator.register("sub");

    let start_time = time::precise_time_ns();
    for argument in env::args().skip(1) {
        let node = parse(&argument);
        // let output = evaluator.evaluate(&node);
        // evaluator.render_markdown(&output);

    }

    println!("Took {} seconds", (time::precise_time_ns() - start_time) as f64 / (1_000_000_000 as f64));
}
