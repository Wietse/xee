// Compile cost on large expressions. Each test runs on its own thread with a
// deadline, so a regression to super-linear time fails instead of hanging
// the suite, and with a large stack, so that debug-build frames cannot turn
// it into a test of stack depth.

use xee_xpath::{Documents, Queries, Query};

fn within_a_minute(f: impl FnOnce() + Send + 'static) {
    within(60, f)
}

fn within(seconds: u64, f: impl FnOnce() + Send + 'static) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            f();
            tx.send(()).unwrap();
        })
        .unwrap();
    match rx.recv_timeout(std::time::Duration::from_secs(seconds)) {
        Ok(()) => {}
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            panic!("did not finish within {seconds} s")
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("the test thread panicked"),
    }
}

// Lowering to IR used to copy the accumulated bindings on every addition,
// so a sequence of n items compiled in quadratic time: 20,000 items took
// 59 s in release, 40,000 took 252 s. This compiles without evaluating:
// evaluating a long sequence literal has a cost of its own.
#[test]
fn a_long_sequence_compiles_in_linear_time() {
    within_a_minute(|| {
        let items = (1..=40_000).map(|i| i.to_string()).collect::<Vec<_>>();
        let src = format!("({})", items.join(", "));
        assert!(Queries::default().sequence(&src).is_ok());
    });
}

// A forward jump's displacement is encoded as an `i16`, but the builder
// checked it against `u16::MAX`, so a displacement between 32,768 and 65,535
// was read back as a jump the other way: an `else if` chain of 2,048 links
// panicked with an index out of bounds. A displacement that does not fit is
// now an implementation limit, XPDY0130.
#[test]
fn a_jump_too_far_is_an_implementation_limit() {
    fn else_if_chain(links: usize) -> String {
        let mut src = String::from("if (true()) then 7 else ");
        for _ in 1..links {
            src.push_str("if (false()) then 1 else ");
        }
        src.push_str("42");
        src
    }
    within_a_minute(|| {
        let queries = Queries::default();
        let src = else_if_chain(2_048);
        let error = queries
            .sequence(&src)
            .expect_err("a chain of 2,048 links needs a jump longer than an i16");
        assert_eq!(error.error, xee_xpath::error::ErrorValue::XPDY0130);
        // The span is that of the `if` whose jump does not fit: compiled on
        // its own it is refused too, while its else branch compiles.
        let span = error.span.expect("a span").range();
        let link = &src[span];
        let else_branch = link
            .strip_prefix("if (false()) then 1 else ")
            .expect("the span starts an inner link");
        let error = queries.sequence(link).expect_err("the link alone");
        assert_eq!(error.error, xee_xpath::error::ErrorValue::XPDY0130);
        assert!(queries.sequence(else_branch).is_ok());

        let query = queries
            .one(&format!("string({})", else_if_chain(1_000)), |_, item| {
                Ok(item.clone())
            })
            .unwrap();
        let mut documents = Documents::new();
        let builder = query.dynamic_context_builder(&documents);
        let context = builder.build();
        let item = query
            .execute_with_context(&mut documents, &context)
            .unwrap();
        assert_eq!(item.to_atomic().unwrap().to_string().unwrap(), "7");
    });
}

// A call's arity is a `u8` in the bytecode. A static call of more than 255
// arguments was refused, but a dynamic one wrapped and ran as a call of
// fewer: it failed with XPTY0004, or, where the argument it then called was
// itself a function, returned a wrong result.
#[test]
fn a_dynamic_call_of_more_than_255_arguments_is_an_implementation_limit() {
    fn call(arity: usize) -> String {
        let params = (0..arity)
            .map(|i| format!("$a{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let args = vec!["0"; arity].join(", ");
        format!("let $f := function({params}) {{ 'right' }} return $f({args})")
    }
    within_a_minute(|| {
        let queries = Queries::default();
        let query = queries.one(&call(255), |_, item| Ok(item.clone())).unwrap();
        let mut documents = Documents::new();
        let builder = query.dynamic_context_builder(&documents);
        let context = builder.build();
        let item = query
            .execute_with_context(&mut documents, &context)
            .unwrap();
        assert_eq!(item.to_atomic().unwrap().to_string().unwrap(), "right");
        for arity in [256, 257] {
            let error = queries.sequence(&call(arity)).expect_err("refused");
            assert_eq!(error.error, xee_xpath::error::ErrorValue::XPDY0130);
        }
    });
}

// Each item of a sequence is held in a local variable, and a local's slot is
// a `u16`: a sequence of 65,535 items compiles, one of 65,536 is refused.
#[test]
fn a_sequence_of_65536_items_is_an_implementation_limit() {
    fn sequence(n: usize) -> String {
        let items = (0..n).map(|i| i.to_string()).collect::<Vec<_>>();
        format!("({})", items.join(", "))
    }
    // Not a test of time: the deadline only keeps a regression to
    // super-linear lowering from hanging the suite (3 s in debug).
    within(600, || {
        let queries = Queries::default();
        assert!(queries.sequence(&sequence(65_535)).is_ok());
        let error = queries
            .sequence(&sequence(65_536))
            .expect_err("65,536 locals do not fit a u16 slot");
        assert_eq!(error.error, xee_xpath::error::ErrorValue::XPDY0130);
    });
}

// `fn:apply` calls a function with an array's members as its arguments, and
// the arity it passes on is a `u8` too: 256 members failed with XPTY0004,
// where the call is an implementation limit, as a dynamic call of 256
// arguments is.
#[test]
fn applying_a_function_to_more_than_255_arguments_is_an_implementation_limit() {
    fn apply(arity: usize) -> String {
        let params = (0..arity)
            .map(|i| format!("$a{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("apply(function({params}) {{ 'right' }}, array {{ (1 to {arity}) ! 0 }})")
    }
    within_a_minute(|| {
        let queries = Queries::default();
        let evaluate = |src: &str| {
            let query = queries.one(src, |_, item| Ok(item.clone())).unwrap();
            let mut documents = Documents::new();
            let builder = query.dynamic_context_builder(&documents);
            let context = builder.build();
            query.execute_with_context(&mut documents, &context)
        };
        let item = evaluate(&apply(255)).unwrap();
        assert_eq!(item.to_atomic().unwrap().to_string().unwrap(), "right");
        for arity in [256, 257] {
            let error = evaluate(&apply(arity)).expect_err("refused");
            assert_eq!(error.error, xee_xpath::error::ErrorValue::XPDY0130);
        }
    });
}
