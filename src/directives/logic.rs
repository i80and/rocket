use parse::Node;
use evaluator::Worker;
use directives::{consume_string, DirectiveHandler};

pub struct If;

impl DirectiveHandler for If {
    fn handle(&self, evaluator: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let mut iter = args.iter();
        let condition = consume_string(&mut iter, evaluator).ok_or(())?;
        let if_true = iter.next().ok_or(())?;
        let if_false = iter.next();

        if iter.next().is_some() {
            return Err(());
        }

        if condition.is_empty() {
            match if_false {
                Some(expr) => Ok(evaluator.evaluate(expr)),
                None => Ok("".to_owned()),
            }
        } else {
            Ok(evaluator.evaluate(if_true))
        }
    }
}

pub struct Not;

impl DirectiveHandler for Not {
    fn handle(&self, evaluator: &mut Worker, args: &[Node]) -> Result<String, ()> {
        if args.len() != 1 {
            return Err(());
        }

        let mut iter = args.iter();
        let value = consume_string(&mut iter, evaluator).ok_or(())?;

        if value.is_empty() {
            Ok("true".to_owned())
        } else {
            Ok("".to_owned())
        }
    }
}

pub struct Equals;

impl DirectiveHandler for Equals {
    fn handle(&self, evaluator: &mut Worker, args: &[Node]) -> Result<String, ()> {
        if args.len() < 2 {
            return Err(());
        }

        let mut iter = args.iter();
        let initial = consume_string(&mut iter, evaluator).ok_or(())?;

        let is_true = iter.all(|node| initial == evaluator.evaluate(node));

        if is_true {
            Ok("true".to_owned())
        } else {
            Ok("".to_owned())
        }
    }
}

pub struct NotEquals;

impl DirectiveHandler for NotEquals {
    fn handle(&self, evaluator: &mut Worker, args: &[Node]) -> Result<String, ()> {
        let equals = Equals;
        let result = equals.handle(evaluator, args)?;

        if result.is_empty() {
            Ok("true".to_owned())
        } else {
            Ok("".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use directives::*;
    use evaluator::Evaluator;


    fn node_string(s: &str) -> Node {
        Node::new_string(s, 0, -1)
    }

    fn node_children(nodes: Vec<Node>) -> Node {
        Node::new_children(nodes, 0, -1)
    }

    #[test]
    fn test_if() {
        let mut evaluator = Evaluator::new();
        evaluator.register_prelude("concat", Box::new(Concat));
        let mut worker = Worker::new(&mut evaluator);
        let handler = If;

        assert!(handler.handle(&mut worker, &[]).is_err());
        assert!(handler.handle(&mut worker, &[node_string("true")]).is_err());
        assert_eq!(
            handler.handle(
                &mut worker,
                &[node_string(""), node_string("true"), node_string("false")]
            ),
            Ok("false".to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_children(vec![node_string("concat"), node_string("foobar")]),
                    node_string("true"),
                    node_string("false")
                ]
            ),
            Ok("true".to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_children(vec![node_string("concat"), node_string("")]),
                    node_string("true"),
                    node_string("false")
                ]
            ),
            Ok("false".to_owned())
        );
    }

    #[test]
    fn test_not() {
        let mut evaluator = Evaluator::new();
        evaluator.register_prelude("concat", Box::new(Concat));
        let mut worker = Worker::new(&mut evaluator);
        let handler = Not;

        assert!(handler.handle(&mut worker, &[]).is_err());
        assert!(
            handler
                .handle(&mut worker, &[node_string("foo"), node_string("bar")])
                .is_err()
        );
        assert_eq!(
            handler.handle(&mut worker, &[node_string("foo")]),
            Ok("".to_owned())
        );
        assert_eq!(
            handler.handle(&mut worker, &[node_string("")]),
            Ok("true".to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_children(vec![node_string("concat"), node_string("foo")])
                ]
            ),
            Ok("".to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut worker,
                &[node_children(vec![node_string("concat"), node_string("")])]
            ),
            Ok("true".to_owned())
        );
    }

    #[test]
    fn test_equals() {
        let mut evaluator = Evaluator::new();
        evaluator.register_prelude("concat", Box::new(Concat));
        let mut worker = Worker::new(&mut evaluator);
        let handler = Equals;

        assert!(handler.handle(&mut worker, &[]).is_err());
        assert!(handler.handle(&mut worker, &[node_string("foo")]).is_err());
        assert_eq!(
            handler.handle(&mut worker, &[node_string("foo"), node_string("foo")]),
            Ok("true".to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_children(vec![node_string("concat"), node_string("foo")]),
                    node_string("foo")
                ]
            ),
            Ok("true".to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_string("foo"),
                    node_children(vec![node_string("concat"), node_string("foo")])
                ]
            ),
            Ok("true".to_owned())
        );
        assert_eq!(
            handler.handle(&mut worker, &[node_string("foo"), node_string("bar")]),
            Ok("".to_owned())
        );
    }

    #[test]
    fn test_not_equals() {
        let mut evaluator = Evaluator::new();
        evaluator.register_prelude("concat", Box::new(Concat));
        let mut worker = Worker::new(&mut evaluator);
        let handler = NotEquals;

        assert!(handler.handle(&mut worker, &[]).is_err());
        assert!(handler.handle(&mut worker, &[node_string("foo")]).is_err());
        assert_eq!(
            handler.handle(&mut worker, &[node_string("foo"), node_string("foo")]),
            Ok("".to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_children(vec![node_string("concat"), node_string("foo")]),
                    node_string("foo")
                ]
            ),
            Ok("".to_owned())
        );
        assert_eq!(
            handler.handle(
                &mut worker,
                &[
                    node_string("foo"),
                    node_children(vec![node_string("concat"), node_string("foo")])
                ]
            ),
            Ok("".to_owned())
        );
        assert_eq!(
            handler.handle(&mut worker, &[node_string("foo"), node_string("bar")]),
            Ok("true".to_owned())
        );
    }
}
