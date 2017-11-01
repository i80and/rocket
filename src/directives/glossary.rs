use directives::{concat_nodes, escape_string, DirectiveHandler};
use evaluator::{RefDef, Worker};
use parse::{Node, NodeValue};

pub struct Glossary;

impl DirectiveHandler for Glossary {
    fn handle(&self, worker: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut result = String::with_capacity(1024);
        result.push_str(r#"<dl class="glossary">"#);

        for node in args {
            let children = match node.value {
                NodeValue::Owned(_) => return Err(()),
                NodeValue::Children(ref children) => children,
            };

            let mut iter = children.iter();
            let term = worker.evaluate(iter.next().ok_or(())?);
            let ref_id = format!("term-{}", escape_string(&term));
            let body = concat_nodes(&mut iter, worker, " ");
            result.push_str(&format!(r#"<dt id="{}">"#, ref_id));
            result.push_str(&term);
            result.push_str("</dt><dd>");
            result.push_str(&body);
            result.push_str("</dd>");

            let refdef = RefDef::new(&term, worker.get_slug());
            worker.insert_refdef(ref_id, refdef);
        }

        result.push_str("</dl>");
        Ok(result)
    }
}
