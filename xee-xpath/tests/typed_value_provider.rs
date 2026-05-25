//! End-to-end tests for the `NodeTypedValueProvider` hook.
//!
//! These exercise the full pipeline: a query compiles, runs through the
//! interpreter, and atomizes a node by consulting a host-supplied
//! provider rather than the schema-unaware untyped fallback.

use std::sync::Arc;

use xee_xpath::{
    context::NodeTypedValueProvider, error::ValueResult, Atomic, Documents, Item, Queries, Query,
    Sequence,
};
use xot::Xot;

/// A provider that reports every node as having an empty typed value —
/// the shape an `xsi:nil="true"` element takes in XBRL.
struct EmptyTypedValueProvider;

impl NodeTypedValueProvider for EmptyTypedValueProvider {
    fn typed_value(&self, _xot: &Xot, _node: xot::Node) -> ValueResult<Option<Vec<Atomic>>> {
        Ok(Some(Vec::new()))
    }
}

/// A provider that reports every node as the integer `7`.
struct IntegerTypedValueProvider;

impl NodeTypedValueProvider for IntegerTypedValueProvider {
    fn typed_value(&self, _xot: &Xot, _node: xot::Node) -> ValueResult<Option<Vec<Atomic>>> {
        Ok(Some(vec![Atomic::from(7i64)]))
    }
}

fn run_with_provider(
    xml: &str,
    xpath: &str,
    provider: Arc<dyn NodeTypedValueProvider>,
) -> xee_xpath::error::Result<Sequence> {
    let mut documents = Documents::new();
    let handle = documents.add_string_without_uri(xml).unwrap();
    let context_item: Item = documents.document_node(handle).unwrap().into();
    let queries = Queries::default();
    let q = queries.sequence(xpath)?;
    q.execute_build_context(&mut documents, move |builder| {
        builder.context_item(context_item);
        builder.typed_value_provider(provider);
    })
}

#[test]
fn value_compare_with_empty_typed_value_returns_empty() {
    // Per XPath 3.1 §3.7.1, a value comparison whose either operand
    // atomizes to the empty sequence returns the empty sequence — not
    // an error. The interpreter already short-circuits when the
    // sequence itself is empty; this test pins the harder case where
    // the sequence holds one node and its *typed value* is empty.
    let provider = Arc::new(EmptyTypedValueProvider);
    let result = run_with_provider("<doc><n/></doc>", "/doc/n gt 0", provider).unwrap();
    assert_eq!(result, Sequence::default());
}

#[test]
fn value_compare_with_typed_value_succeeds() {
    // Same shape as above, but the provider supplies a real value;
    // confirms the provider plumbing is what's at work, not some
    // other path that happens to swallow empty operands.
    let provider = Arc::new(IntegerTypedValueProvider);
    let result = run_with_provider("<doc><n/></doc>", "/doc/n gt 0", provider).unwrap();
    assert_eq!(result, Sequence::from(true));
}

#[test]
fn value_compare_empty_on_either_side_returns_empty() {
    // Both `empty op non-empty` and `non-empty op empty` must yield
    // empty — neither order should raise XPTY0004.
    let lhs_empty = run_with_provider(
        "<doc><n/></doc>",
        "/doc/n eq 1",
        Arc::new(EmptyTypedValueProvider),
    )
    .unwrap();
    assert_eq!(lhs_empty, Sequence::default());
    let rhs_empty = run_with_provider(
        "<doc><n/></doc>",
        "1 eq /doc/n",
        Arc::new(EmptyTypedValueProvider),
    )
    .unwrap();
    assert_eq!(rhs_empty, Sequence::default());
}
