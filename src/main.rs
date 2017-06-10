extern crate argparse;
extern crate comrak;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate simple_logger;
extern crate regex;
extern crate time;

mod directives;
mod evaluator;
mod lex;
mod parse;

use argparse::{ArgumentParser, StoreTrue};
use parse::parse;
use evaluator::Evaluator;

fn main() {
    let mut option_verbose = false;
    let mut option_inputs: Vec<String> = vec![];

    {
        let mut ap = ArgumentParser::new();
        ap.set_description("The Rocket documentation build system.");
        ap.refer(&mut option_verbose)
            .add_option(&["-v", "--verbose"], StoreTrue, "Be verbose");
        ap.refer(&mut option_inputs)
            .add_argument("inputs", argparse::List, "Files to compile");
        ap.parse_args_or_exit();
    }

    let loglevel = match option_verbose {
        true => log::LogLevel::Debug,
        false => log::LogLevel::Info,
    };
    simple_logger::init_with_level(loglevel).expect("Failed to initialize logger");

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
    evaluator.register("include", Box::new(directives::Include::new()));

    let start_time = time::precise_time_ns();
    for argument in option_inputs {
        let node = match parse(&argument) {
            Ok(n) => n,
            Err(_) => {
                continue;
            }
        };
        let output = evaluator.evaluate(&node);
        println!("{}", output);

    }

    println!("Took {} seconds",
             (time::precise_time_ns() - start_time) as f64 / (1_000_000_000 as f64));
}
