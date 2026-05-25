//! End-to-end tests for the `NodeNilledProvider` hook.
//!
//! Mirror of `typed_value_provider.rs`. With no provider, `fn:nilled`
//! returns `false` for every element (the schema-unaware default that
//! the QT3 conformance suite already exercises); a host that knows
//! the PSVI `[nilled]` property overrides per-node.

use std::sync::Arc;

use xee_xpath::{context::NodeNilledProvider, Documents, Item, Queries, Query, Sequence};
use xot::Xot;

/// A provider that reports every element as nilled — what an XBRL
/// host installing the wire-format `xsi:nil="true"` shortcut would
/// look like for a document where every element is nilled.
struct AlwaysNilled;

impl NodeNilledProvider for AlwaysNilled {
    fn nilled(&self, _xot: &Xot, _node: xot::Node) -> Option<bool> {
        Some(true)
    }
}

/// A provider that has no opinion on any node — exercises the
/// `None → fall back to default` arm.
struct DefersToDefault;

impl NodeNilledProvider for DefersToDefault {
    fn nilled(&self, _xot: &Xot, _node: xot::Node) -> Option<bool> {
        None
    }
}

fn run_with_provider(
    xml: &str,
    xpath: &str,
    provider: Arc<dyn NodeNilledProvider>,
) -> xee_xpath::error::Result<Sequence> {
    let mut documents = Documents::new();
    let handle = documents.add_string_without_uri(xml).unwrap();
    let context_item: Item = documents.document_node(handle).unwrap().into();
    let queries = Queries::default();
    let q = queries.sequence(xpath)?;
    q.execute_build_context(&mut documents, move |builder| {
        builder.context_item(context_item);
        builder.nilled_provider(provider);
    })
}

#[test]
fn provider_overrides_default_for_element() {
    let result = run_with_provider(
        r#"<doc><n xsi:nil="true" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"/></doc>"#,
        "fn:nilled(/doc/n)",
        Arc::new(AlwaysNilled),
    )
    .unwrap();
    assert_eq!(result, Sequence::from(true));
}

#[test]
fn provider_returning_none_defers_to_default() {
    // Provider opts out of opining on this node → xee's default
    // (schema-unaware: not nilled) is reported.
    let result = run_with_provider(
        "<doc><n/></doc>",
        "fn:nilled(/doc/n)",
        Arc::new(DefersToDefault),
    )
    .unwrap();
    assert_eq!(result, Sequence::from(false));
}

#[test]
fn provider_not_consulted_for_non_element_argument() {
    // `fn:nilled` on a text node returns the empty sequence per
    // spec; the provider must not be reached.
    let result = run_with_provider(
        "<doc>text</doc>",
        "fn:nilled(/doc/text())",
        Arc::new(AlwaysNilled),
    )
    .unwrap();
    assert_eq!(result, Sequence::default());
}

#[test]
fn provider_not_consulted_for_empty_argument() {
    let result = run_with_provider("<doc/>", "fn:nilled(())", Arc::new(AlwaysNilled)).unwrap();
    assert_eq!(result, Sequence::default());
}
