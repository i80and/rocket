extern crate comrak;
#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate time;

mod directives;
mod evaluator;
mod lex;
mod parse;

use std::env;
use parse::parse;
use evaluator::Evaluator;

fn main() {
    let mut evaluator = Evaluator::new();
    evaluator.register("md", Box::new(directives::Markdown::new()));
    evaluator.register("table", Box::new(directives::Dummy::new()));
    evaluator.register("version", Box::new(directives::Version::new("3.4.0")));
    evaluator.register("note",
                       Box::new(directives::Admonition::new("Note", "note")));
    evaluator.register("warning",
                       Box::new(directives::Admonition::new("Warning", "warning")));
    evaluator.register("insert", Box::new(directives::Dummy::new()));
    evaluator.register("manual",
                       Box::new(directives::LinkTemplate::new("https://docs.mongodb.com/manual")));
    evaluator.register("definition-list",
                       Box::new(directives::DefinitionList::new()));

    evaluator.register("concat", Box::new(directives::Concat::new()));

    let start_time = time::precise_time_ns();
    for argument in env::args().skip(1) {
        let node = parse(&argument);
        let output = evaluator.evaluate(&node);
        println!("{}", output);

    }

    println!("Took {} seconds",
             (time::precise_time_ns() - start_time) as f64 / (1_000_000_000 as f64));
}
