//! More than 65,536 distinct names, namespaces and prefixes in one `Xot`:
//! each keeps its own id. With 16-bit ids the 65,537th wrapped and read
//! back as another.

use xot::Xot;

const N: usize = 70_000;

#[test]
fn test_more_than_65536_names() {
    let mut xml = String::from("<r>");
    for i in 0..N {
        xml.push_str(&format!("<e{i}/>"));
    }
    xml.push_str("<last/></r>");
    let mut xot = Xot::new();
    let root = xot.parse(&xml).unwrap();
    let r = xot.document_element(root).unwrap();
    let children: Vec<_> = xot.children(r).collect();
    assert_eq!(children.len(), N + 1);
    for (i, child) in children.iter().take(N).enumerate() {
        assert_eq!(
            xot.local_name_str(xot.node_name(*child).unwrap()),
            format!("e{i}")
        );
    }
    assert_eq!(
        xot.local_name_str(xot.node_name(children[N]).unwrap()),
        "last"
    );
    assert_eq!(xot.to_string(root).unwrap(), xml);
}

#[test]
fn test_more_than_65536_namespaces() {
    let mut xml = String::from("<r>");
    for i in 0..N {
        xml.push_str(&format!(r#"<x:e xmlns:x="urn:{i}"/>"#));
    }
    xml.push_str("</r>");
    let mut xot = Xot::new();
    let root = xot.parse(&xml).unwrap();
    let r = xot.document_element(root).unwrap();
    for (i, child) in xot.children(r).enumerate() {
        assert_eq!(
            xot.uri_str(xot.node_name(child).unwrap()),
            format!("urn:{i}")
        );
    }
    assert_eq!(xot.to_string(root).unwrap(), xml);
}

#[test]
fn test_more_than_65536_prefixes() {
    let mut xml = String::from(r#"<r>"#);
    for i in 0..N {
        xml.push_str(&format!(r#"<p{i}:e xmlns:p{i}="urn:a"/>"#));
    }
    xml.push_str("</r>");
    let mut xot = Xot::new();
    let root = xot.parse(&xml).unwrap();
    let r = xot.document_element(root).unwrap();
    for (i, child) in xot.children(r).enumerate() {
        let (prefix, _) = xot.namespaces(child).iter().next().unwrap();
        assert_eq!(xot.prefix_str(prefix), format!("p{i}"));
    }
    assert_eq!(xot.to_string(root).unwrap(), xml);
}
