//! fn:serialize of an element below the root: the namespace declarations
//! it inherits are written in a fixed order, farthest ancestor first, each
//! ancestor's in its own order, so that a name takes the prefix a
//! serialization of the whole document gives it.

mod common;

use common::run_xml;

#[test]
fn test_inherited_declarations_in_declaration_order() {
    let xml = concat!(
        r#"<r xmlns:f="urn:f" xmlns:b="urn:b">"#,
        r#"<m xmlns:e="urn:e" xmlns:a="urn:a" xmlns:d="urn:d" xmlns:c="urn:c">"#,
        r#"<f:x b:y="1"><e:z a:w="2"/><d:v c:u="3"/></f:x></m></r>"#
    );
    let expected = concat!(
        r#"<f:x xmlns:f="urn:f" xmlns:b="urn:b" xmlns:e="urn:e" xmlns:a="urn:a""#,
        r#" xmlns:d="urn:d" xmlns:c="urn:c" b:y="1"><e:z a:w="2"/><d:v c:u="3"/></f:x>"#
    );
    // a hash map's order would match this by chance once in 720; run it a
    // few times, each with a map of its own
    for _ in 0..4 {
        let result = run_xml(xml, &format!("serialize(/*/*/*) eq '{expected}'")).unwrap();
        assert!(
            result.effective_boolean_value().unwrap(),
            "{:?}",
            run_xml(xml, "serialize(/*/*/*)").unwrap()
        );
    }
}

#[test]
fn test_the_nearer_prefix_of_one_namespace() {
    let xml = r#"<r xmlns:a="urn:x"><m xmlns:b="urn:x"><b:x b:p="1"/></m></r>"#;
    let result = run_xml(
        xml,
        r#"serialize(/*/*/*) eq '<b:x xmlns:a="urn:x" xmlns:b="urn:x" b:p="1"/>'"#,
    )
    .unwrap();
    assert!(
        result.effective_boolean_value().unwrap(),
        "{:?}",
        run_xml(xml, "serialize(/*/*/*)").unwrap()
    );
}
