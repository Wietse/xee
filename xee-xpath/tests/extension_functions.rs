//! End-to-end tests for registered extension functions.
//!
//! The registry's unit tests (in `xee-interpreter`) cover registration
//! and name/id resolution. These cover the remaining link: an XPath
//! expression that *calls* a host-registered function is compiled and
//! evaluated, and the function's body actually runs inside the
//! interpreter and returns a result.
//!
//! `extension_function_reads_user_data` additionally exercises the
//! `DynamicContext` user-data slot from inside an extension function —
//! the path a real host (e.g. an XBRL engine reaching a DTS handle)
//! would use.

use std::sync::Arc;

use ibig::{ibig, IBig};
use xee_interpreter::{context::DynamicContext, wrap_xpath_fn};
use xee_xpath::{
    context::{StaticContext, StaticContextBuilder},
    Documents, Queries, Query,
};
use xee_xpath_macros::xpath_fn;

#[xpath_fn("Q{http://example.com/xee-test}add($a as xs:integer, $b as xs:integer) as xs:integer")]
fn ext_add(a: IBig, b: IBig) -> IBig {
    a + b
}

struct HostState {
    answer: i64,
}

#[xpath_fn("Q{http://example.com/xee-test}host-answer() as xs:integer")]
fn ext_host_answer(context: &DynamicContext) -> IBig {
    let state = context
        .user_data::<HostState>()
        .expect("host state must be registered on the dynamic context");
    IBig::from(state.answer)
}

fn registry_context() -> StaticContext {
    let mut builder = StaticContextBuilder::default();
    builder.add_functions([wrap_xpath_fn!(ext_add), wrap_xpath_fn!(ext_host_answer)]);
    builder.build().expect("extension registry should build")
}

#[test]
fn extension_function_is_invoked() {
    let queries = Queries::default();
    let q = queries
        .one_with_context(
            "Q{http://example.com/xee-test}add(2, 3)",
            |_, item| {
                let v: IBig = item.to_atomic()?.try_into()?;
                Ok(v)
            },
            registry_context(),
        )
        .expect("query should compile");

    let mut documents = Documents::new();
    let result = q
        .execute_build_context(&mut documents, |_| {})
        .expect("query should execute");

    assert_eq!(result, ibig!(5));
}

#[test]
fn extension_function_reads_user_data() {
    let queries = Queries::default();
    let q = queries
        .one_with_context(
            "Q{http://example.com/xee-test}host-answer()",
            |_, item| {
                let v: IBig = item.to_atomic()?.try_into()?;
                Ok(v)
            },
            registry_context(),
        )
        .expect("query should compile");

    let mut documents = Documents::new();
    let result = q
        .execute_build_context(&mut documents, |builder| {
            builder.user_data(Arc::new(HostState { answer: 42 }));
        })
        .expect("query should execute");

    assert_eq!(result, ibig!(42));
}
