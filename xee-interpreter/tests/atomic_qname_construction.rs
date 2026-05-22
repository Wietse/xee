//! Locks that an `xs:QName` [`Atomic`] can be constructed from outside
//! `xee-interpreter` — the construction path the host-provided
//! typed-node-value provider relies on (see the `xbrl-typed-nodes`
//! branch / `wja_xbrl-typed-nodes.md`).
//!
//! As an integration test this compiles as its own crate, so it only
//! sees `xee-interpreter`'s public API — the same surface an external
//! host crate sees.

use xee_interpreter::atomic::{Atomic, Name};

#[test]
fn qname_atomic_constructible_from_external_crate() {
    // `Name` is re-exported, and `Name::new` + `From<Name> for Atomic`
    // are the public path a host uses to build an `xs:QName` — e.g.
    // for the typed value of an `<xbrli:measure>` element.
    let name = Name::new(
        "EUR".to_string(),
        "http://www.xbrl.org/2003/iso4217".to_string(),
        String::new(),
    );
    let atomic: Atomic = name.into();
    assert!(matches!(atomic, Atomic::QName(_)));
}
