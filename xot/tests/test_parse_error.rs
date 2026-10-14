use xot::Xot;

#[test]
fn test_parse_invalid_close_tag() {
    let xml = r#"<doc></a></doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidCloseTag { .. }));
    assert_eq!(err.span(), (7..8).into());
}

#[test]
fn test_parse_invalid_close_tag_prefix() {
    let xml = r#"<doc xmlns:a="http://example.com"></a:doc></doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidCloseTag { .. }));
    assert_eq!(err.span(), (36..41).into());
}

#[test]
fn test_unknown_prefix() {
    let xml = r#"<doc><a:p/></doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::UnknownPrefix { .. }));
    assert_eq!(err.span(), (6..7).into());
}

#[test]
fn test_unsupported_version() {
    let xml = r#"<?xml version="1.1"?><doc/></doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::UnsupportedVersion { .. }));
    assert_eq!(err.span(), (15..18).into());
}

// The order of the version, encoding and standalone attributes in the XML declaration is fixed by
// the standard.
#[test]
fn test_parse_invalid_xml_declaration() {
    let mut xot = Xot::new();
    let err = xot
        .parse(r#"<?xml version="1.0" standalone="yes" encoding="UTF-8"?><a/>"#)
        .unwrap_err();
    assert!(matches!(err, xot::ParseError::XmlParser { .. }));
    match err {
        xot::ParseError::XmlParser(e, _) => {
            assert!(matches!(e, xmlparser::Error::InvalidDeclaration { .. }))
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_xmlparser_error() {
    let xml = r#"<doc><"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::XmlParser { .. }));
    assert_eq!(err.span(), (5..5).into());
}

#[test]
fn test_duplicate_attribute() {
    let xml = r#"<doc a="1" a="2"/>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::DuplicateAttribute { .. }));
    assert_eq!(err.span(), (11..12).into());
}

#[test]
fn test_duplicate_attribute_with_prefix() {
    let xml = r#"<doc xmlns:a="http://example.com" a:b="1" a:b="2"/>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::DuplicateAttribute { .. }));
    assert_eq!(err.span(), (42..45).into());
}

#[test]
fn test_invalid_dtd() {
    let xml = r#"<!DOCTYPE note><note></note>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::DtdUnsupported { .. }));
    assert_eq!(err.span(), (0..15).into());
}

#[test]
fn test_dtd_unsupported() {
    let xml = r#"<!DOCTYPE note SYSTEM "Note.dtd"><note></note>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::DtdUnsupported { .. }));
    assert_eq!(err.span(), (0..33).into());
}

#[test]
fn test_dtd_unsupported2() {
    let xml = r#"<?xml version="1.0"?><!DOCTYPE note SYSTEM "Note.dtd"><note></note>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::DtdUnsupported { .. }));
    assert_eq!(err.span(), (21..54).into());
}

#[test]
fn test_unclosed_tag() {
    let xml = r#"<doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::UnclosedTag(_)));
    assert_eq!(err.span(), (1..4).into());
}

#[test]
fn test_unclosed_tag_middle() {
    let xml = r#"<doc><a></doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidCloseTag(_, _, _)));
    assert_eq!(err.span(), (10..13).into());
}

#[test]
fn test_unclosed_tag_with_prefix() {
    let xml = r#"<a:doc xmlns:a="http://example.com">"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::UnclosedTag(_)));
    assert_eq!(err.span(), (1..6).into());
}

#[test]
fn test_only_text_top_level() {
    let xml = r#"abc"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::XmlParser { .. }));
    assert_eq!(err.span(), (0..0).into());
}

#[test]
fn test_only_comment_top_level() {
    let xml = r#"<!-- foo -->"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::NoElementAtTopLevel { .. }));
    assert_eq!(err.span(), (12..12).into());
}

#[test]
fn test_unclosed_entity() {
    let xml = r#"<doc>&foo"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::UnclosedEntity(_, _)));
    assert_eq!(err.span(), (5..5).into());
}

#[test]
fn test_invalid_entity() {
    let xml = r#"<doc>&foo;</doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidEntity(_, _)));
    assert_eq!(err.span(), (5..10).into());
}

// the duplicate check is a set of the qualified names seen in the start
// tag: a repeat is found after many distinct attributes, and the same
// local name under another prefix is not a repeat
#[test]
fn test_duplicate_attribute_after_many() {
    let mut xml = String::from(r#"<doc xmlns:x="urn:x""#);
    for i in 0..20_000 {
        xml.push_str(&format!(r#" a{i}="v""#));
    }
    let tail = xml.len() + 1;
    xml.push_str(r#" a7="w"/>"#);
    let mut xot = Xot::new();
    let err = xot.parse(&xml).unwrap_err();
    match &err {
        xot::ParseError::DuplicateAttribute(name, _) => assert_eq!(name, "a7"),
        _ => panic!("{err:?}"),
    }
    assert_eq!(err.span(), (tail..tail + 2).into());
}

// what is and is not a repeat, below the scan limit and above it, where
// `pad` puts 40 other attributes first: the same local name under another
// prefix, a prefix and name that spell another name run together, and
// another case are not repeats; a repeat after the limit is
#[test]
fn test_what_a_repeated_attribute_is() {
    let pad: String = (0..40).map(|i| format!(r#" p{i}="v""#)).collect();
    for pad in ["", pad.as_str()] {
        for tail in [
            r#" a="1" x:a="2" y:a="3""#,
            r#" x:ab="1" xa:b="2""#,
            r#" a="1" A="2""#,
        ] {
            let xml =
                format!(r#"<doc xmlns:x="urn:x" xmlns:y="urn:y" xmlns:xa="urn:xa"{pad}{tail}/>"#);
            let mut xot = Xot::new();
            assert!(xot.parse(&xml).is_ok(), "{xml}");
        }
        let xml = format!(r#"<doc xmlns:x="urn:x"{pad} x:a="1" x:a="2"/>"#);
        let mut xot = Xot::new();
        let err = xot.parse(&xml).unwrap_err();
        match &err {
            xot::ParseError::DuplicateAttribute(name, _) => assert_eq!(name, "x:a", "{xml}"),
            _ => panic!("{xml}: {err:?}"),
        }
    }
}

// a repeat of an attribute from before the switch to the set is found,
// the first and the last before it alike
#[test]
fn test_duplicate_attribute_across_the_scan_limit() {
    let attributes: String = (0..32).map(|i| format!(r#" a{i}="v""#)).collect();
    for repeat in ["a0", "a31"] {
        let xml = format!(r#"<doc{attributes} {repeat}="w"/>"#);
        let mut xot = Xot::new();
        let err = xot.parse(&xml).unwrap_err();
        match &err {
            xot::ParseError::DuplicateAttribute(name, _) => assert_eq!(name, repeat),
            _ => panic!("{err:?}"),
        }
    }
}
