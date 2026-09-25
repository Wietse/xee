//! fn:serialize with the JSON output method (Serialization 3.1 §9).
//!
//! XPath string literals have no escapes, so control and non-BMP
//! characters are built with `codepoints-to-string`. Expected JSON escapes
//! are built with `esc`, and expected non-ASCII characters with `ch`,
//! never written out as escape text in the source.

mod common;

use common::run;
use xee_xpath::{error::ErrorValue, Documents, Queries, Query};

fn assert_true(expr: &str) {
    let result = run(expr).unwrap_or_else(|e| panic!("{expr}: {e:?}"));
    assert!(
        result.effective_boolean_value().unwrap(),
        "{expr} should be true"
    );
}

fn error_of(expr: &str) -> ErrorValue {
    run(expr).expect_err(expr).error
}

// Values outside a method parameter's domain are SEPM0016 when the
// parameters are read, even if the parameter is never used: here no node is
// serialized, so json-node-output-method never applies.
#[test]
fn test_method_values_outside_the_domain_are_sepm0016() {
    for value in ["'json'", "'adaptive'", "'foo'", "''"] {
        let expr =
            format!("serialize(1, map{{'method':'json','json-node-output-method':{value}}})");
        assert_eq!(error_of(&expr), ErrorValue::SEPM0016, "{expr}");
    }
    for value in ["'foo'", "''"] {
        let expr = format!("serialize(1, map{{'method':{value}}})");
        assert_eq!(error_of(&expr), ErrorValue::SEPM0016, "{expr}");
    }
}

#[test]
fn test_text_output_method() {
    assert_true(
        "serialize(parse-xml('<a>x<b>y</b></a>'), \
         map{'method':'json','json-node-output-method':'text'}) eq '\"xy\"'",
    );
    assert_true("serialize(parse-xml('<a>x<b>y</b></a>'), map{'method':'text'}) eq 'xy'");
    assert_true("serialize((1, 2), map{'method':'text', 'item-separator':'-'}) eq '1-2'");
}

// xhtml and adaptive are valid methods that Xee does not implement, and a
// QName in a namespace names an implementation-defined method, of which Xee
// has none. None of these is an invalid parameter value.
#[test]
fn test_valid_but_unimplemented_methods_are_unsupported() {
    for value in ["'xhtml'", "QName('http://example.com/ns', 'xml')"] {
        let expr = format!(
            "serialize(parse-xml('<a/>'), map{{'method':'json','json-node-output-method':{value}}})"
        );
        assert!(
            matches!(error_of(&expr), ErrorValue::Unsupported(_)),
            "{expr}"
        );
    }
    for value in [
        "'xhtml'",
        "'adaptive'",
        "QName('http://example.com/ns', 'x')",
    ] {
        let expr = format!("serialize(parse-xml('<a/>'), map{{'method':{value}}})");
        assert!(
            matches!(error_of(&expr), ErrorValue::Unsupported(_)),
            "{expr}"
        );
    }
}

#[test]
fn test_xml_and_html_json_node_output_methods_still_serialize() {
    // Here we only check that xml and html keep serializing the node; the
    // escaping of the result is `test_node_output_is_escaped` below.
    for method in ["xml", "html"] {
        assert_true(&format!(
            "serialize(parse-xml('<a/>'), map{{'method':'json','json-node-output-method':'{method}'}}) => contains('<a')"
        ));
    }
}

// The JSON output method on the xee-json writer.

/// The one string an expression returns.
fn string_of(expr: &str) -> String {
    let mut documents = Documents::new();
    let queries = Queries::default();
    let q = queries
        .one(expr, |_, item| Ok(item.try_into_value::<String>()?))
        .unwrap_or_else(|e| panic!("{expr}: {e:?}"));
    q.execute_build_context(&mut documents, |_| ())
        .unwrap_or_else(|e| panic!("{expr}: {e:?}"))
}

/// `serialize(arg, map{'method':'json', params})`.
fn json_expr(arg: &str, params: &str) -> String {
    if params.is_empty() {
        format!("serialize({arg}, map{{'method':'json'}})")
    } else {
        format!("serialize({arg}, map{{'method':'json',{params}}})")
    }
}

fn json(arg: &str, params: &str) -> String {
    string_of(&json_expr(arg, params))
}

fn json_error(arg: &str, params: &str) -> ErrorValue {
    error_of(&json_expr(arg, params))
}

/// The six-character JSON escape with the given hexadecimal digits.
fn esc(hex: &str) -> String {
    format!("{}u{}", '\\', hex)
}

fn ch(codepoint: u32) -> char {
    char::from_u32(codepoint).unwrap()
}

fn quoted(inner: &str) -> String {
    format!("\"{inner}\"")
}

// §9: the solidus and U+007F..U+009F are escaped, in values and keys.
#[test]
fn test_solidus_and_delete_and_c1_controls_are_escaped() {
    assert_eq!(json("'a/b'", ""), r#""a\/b""#);
    assert_eq!(json("map{'/':'/'}", ""), r#"{"\/":"\/"}"#);
    assert_eq!(
        json("xs:anyURI('http://www.w3.org/')", ""),
        r#""http:\/\/www.w3.org\/""#
    );
    for (codepoint, hex) in [(127, "007F"), (128, "0080"), (150, "0096"), (159, "009F")] {
        let text = format!("codepoints-to-string({codepoint})");
        assert_eq!(json(&text, ""), quoted(&esc(hex)), "{codepoint}");
        assert_eq!(
            json(&format!("map{{{text}:1}}"), ""),
            format!("{{{}:1}}", quoted(&esc(hex))),
            "{codepoint}"
        );
    }
    // The neighbours of the range stay literal.
    assert_eq!(
        json("codepoints-to-string((126, 160))", ""),
        quoted(&format!("~{}", ch(160)))
    );
    // The two-character forms. The other controls below 32 cannot occur in
    // an xs:string (XML 1.0).
    assert_eq!(
        json("codepoints-to-string((9, 10, 13, 34, 92))", ""),
        quoted(r#"\t\n\r\"\\"#)
    );
}

// §9: a node becomes a JSON string, so its serialization is escaped like
// any other string (QT3 serialize-json-009b and -127).
#[test]
fn test_node_output_is_escaped() {
    assert_eq!(json("parse-xml('<a>b</a>')", ""), r#""<a>b<\/a>""#);
    assert_eq!(
        json(
            "parse-xml('<a>&#127;/</a>')",
            "'json-node-output-method':'text'"
        ),
        quoted(&format!(r"{}\/", esc("007F")))
    );
}

// §9 allows any RFC 8259 form of a number; Xee writes the canonical
// xs:string form (F&O 3.1 §19.1.2), which keeps integers and decimals
// exact.
#[test]
fn test_numbers_keep_their_exact_text() {
    let big = format!("1{}", "0".repeat(30));
    assert_eq!(json(&format!("xs:integer('{big}')"), ""), big);
    assert_eq!(
        json(&format!("xs:integer('-{big}')"), ""),
        format!("-{big}")
    );
    assert_eq!(
        json(
            "[0, -5, xs:byte(7), xs:unsignedLong('18446744073709551615')]",
            ""
        ),
        "[0,-5,7,18446744073709551615]"
    );
    // Decimals that f64 would round.
    assert_eq!(json("12345678901234567.89", ""), "12345678901234567.89");
    assert_eq!(
        json("0.1234567890123456789012345678", ""),
        "0.1234567890123456789012345678"
    );
    assert_eq!(
        json("79228162514264337593543950335.0", ""),
        "79228162514264337593543950335"
    );
    assert_eq!(
        json("-7922816251426433759354395033.5", ""),
        "-7922816251426433759354395033.5"
    );
    assert_eq!(json("-1.50", ""), "-1.5");
    // xs:float and xs:double.
    assert_eq!(json("xs:float('0.1')", ""), "0.1");
    assert_eq!(json("xs:double('0.1')", ""), "0.1");
    assert_eq!(json("-0e0", ""), "-0");
    assert_eq!(json("xs:float('-0')", ""), "-0");
    assert_eq!(json("0e0", ""), "0");
    assert_eq!(json("1.5e30", ""), "1.5E30");
    assert_eq!(json("12.34e-30", ""), "1.234E-29");
    assert_eq!(json("1e6", ""), "1.0E6");
    // Every one reads back as the same value.
    for arg in [
        "xs:integer('-1000000000000000000000000000000')",
        "12345678901234567.89",
        "1.5e30",
        "-0e0",
    ] {
        assert_true(&format!(
            "let $v := {arg} return parse-json({}) eq $v",
            json_expr("$v", "")
        ));
    }
}

#[test]
fn test_infinity_and_nan_are_sere0020() {
    for arg in [
        "xs:double('INF')",
        "xs:double('-INF')",
        "xs:double('NaN')",
        "xs:float('INF')",
        "xs:float('-INF')",
        "xs:float('NaN')",
        "[1, xs:double('NaN')]",
        "map{'a':xs:float('INF')}",
    ] {
        assert_eq!(json_error(arg, ""), ErrorValue::SERE0020, "{arg}");
    }
}

// §9 and §9.1.16: keys with the same string value are SERE0022, unless
// allow-duplicate-names is true.
#[test]
fn test_keys_with_the_same_string_value_are_sere0022() {
    for arg in [
        // §9.1.16's example.
        "map{xs:date('2014-10-01'):1, '2014-10-01':2}",
        // QT3 serialize-json-010.
        "map{xs:QName('foo'):1, 'foo':2}",
        "map{1:1, '1':2}",
        "[map{1:1, '1':2}]",
        "map{'a':map{1:1, '1':2}}",
    ] {
        assert_eq!(json_error(arg, ""), ErrorValue::SERE0022, "{arg}");
        assert_eq!(
            json_error(arg, "'allow-duplicate-names':false()"),
            ErrorValue::SERE0022,
            "{arg}"
        );
    }
    assert_eq!(json("map{1:1}", ""), r#"{"1":1}"#);
}

#[test]
fn test_allow_duplicate_names_writes_both_members() {
    let arg = "map{xs:date('2014-10-01'):1, '2014-10-01':2}";
    let out = json(arg, "'allow-duplicate-names':true()");
    assert!(
        out == r#"{"2014-10-01":1,"2014-10-01":2}"# || out == r#"{"2014-10-01":2,"2014-10-01":1}"#,
        "{out}"
    );
    let out = json(&format!("[{arg}]"), "'allow-duplicate-names':true()");
    assert_eq!(out.matches("2014-10-01").count(), 2, "{out}");
    // QT3 serialize-json-011's second assertion.
    assert_true(&format!(
        "map:size(parse-json({})) eq 1",
        json_expr(
            "map{QName('', 'foo'):1, 'foo':2}",
            "'allow-duplicate-names':true()"
        )
    ));
}

// §9.1.4: indent yes may add whitespace next to structural tokens; indent
// no must add none.
#[test]
fn test_indent() {
    let arg = "map{'a':[1, 'x y', [], map{'b':()}]}";
    let compact = r#"{"a":[1,"x y",[],{"b":null}]}"#;
    assert_eq!(json(arg, ""), compact);
    assert_eq!(json(arg, "'indent':false()"), compact);
    let indented = [
        "{",
        r#"  "a": ["#,
        "    1,",
        r#"    "x y","#,
        "    [],",
        "    {",
        r#"      "b": null"#,
        "    }",
        "  ]",
        "}",
    ]
    .join("\n");
    assert_eq!(json(arg, "'indent':true()"), indented);
    // Indented output reads back as the same value.
    let value = "map{'a':[1, 'x y', [], map{'b':(), 'c':[true(), 'd']}], 'e':12.5}";
    for indent in ["true()", "false()"] {
        assert_true(&format!(
            "deep-equal(parse-json({}), {value})",
            json_expr(value, &format!("'indent':{indent}"))
        ));
    }
}

// §9: characters the encoding cannot represent are escaped; §9.1.3:
// UTF-8 and UTF-16 are required, an unsupported encoding is SESU0007.
#[test]
fn test_encoding_escapes_what_it_cannot_represent() {
    // x, U+00E9, U+0100, U+1D11E
    let text = "codepoints-to-string((120, 233, 256, 119070))";
    let all = quoted(&format!("x{}{}{}", ch(233), ch(256), ch(119070)));
    assert_eq!(json(text, ""), all);
    for encoding in ["utf-8", "UTF-8", "UTF-16", "utf-32"] {
        let params = format!("'encoding':'{encoding}'");
        assert_eq!(json(text, &params), all, "{encoding}");
    }
    let pair = format!("{}{}", esc("D834"), esc("DD1E"));
    let ascii = quoted(&format!("x{}{}{pair}", esc("00E9"), esc("0100")));
    for encoding in ["US-ASCII", "us-ascii", "ASCII"] {
        let params = format!("'encoding':'{encoding}'");
        assert_eq!(json(text, &params), ascii, "{encoding}");
    }
    let latin1 = quoted(&format!("x{}{}{pair}", ch(233), esc("0100")));
    for encoding in ["ISO-8859-1", "iso-8859-1", "latin1"] {
        let params = format!("'encoding':'{encoding}'");
        assert_eq!(json(text, &params), latin1, "{encoding}");
    }
    // Keys and node output too.
    assert_eq!(
        json("map{codepoints-to-string(233):1}", "'encoding':'US-ASCII'"),
        format!("{{{}:1}}", quoted(&esc("00E9")))
    );
    assert_eq!(
        json(
            "parse-xml('<a>&#233;</a>')",
            "'json-node-output-method':'text','encoding':'US-ASCII'"
        ),
        quoted(&esc("00E9"))
    );
}

#[test]
fn test_unsupported_encoding_is_sesu0007() {
    for encoding in ["EBCDIC-US", "x-unknown", ""] {
        for arg in ["'a'", "1", "()"] {
            let params = format!("'encoding':'{encoding}'");
            assert_eq!(
                json_error(arg, &params),
                ErrorValue::SESU0007,
                "{encoding} {arg}"
            );
        }
    }
}

// §9.1.9: NFC and none must be supported. Normalization comes before
// escaping (§9), and applies to keys and node output as well.
#[test]
fn test_normalization_form() {
    let decomposed = "concat('e', codepoints-to-string(769))";
    let composed = "codepoints-to-string(233)";
    let d = quoted(&format!("e{}", ch(769)));
    let c = quoted(&ch(233).to_string());
    assert_eq!(json(decomposed, "'normalization-form':'NFC'"), c);
    assert_eq!(json(decomposed, "'normalization-form':'none'"), d);
    assert_eq!(json(decomposed, ""), d);
    assert_eq!(json(composed, "'normalization-form':'NFD'"), d);
    assert_eq!(json(composed, "'normalization-form':'none'"), c);
    // U+FB01, the fi ligature, has only a compatibility decomposition.
    let ligature = "codepoints-to-string(64257)";
    assert_eq!(json(ligature, "'normalization-form':'NFKC'"), quoted("fi"));
    assert_eq!(json(ligature, "'normalization-form':'NFKD'"), quoted("fi"));
    assert_eq!(
        json(ligature, "'normalization-form':'NFC'"),
        quoted(&ch(64257).to_string())
    );
    assert_eq!(
        json(
            &format!("map{{{decomposed}:1}}"),
            "'normalization-form':'NFC'"
        ),
        format!("{{{c}:1}}")
    );
    assert_eq!(
        json(
            "parse-xml('<a>e&#769;</a>')",
            "'json-node-output-method':'text','normalization-form':'NFC'"
        ),
        c
    );
    // Normalized first, then escaped for the encoding.
    assert_eq!(
        json(
            decomposed,
            "'normalization-form':'NFC','encoding':'US-ASCII'"
        ),
        quoted(&esc("00E9"))
    );
}

// SERE0022 compares the keys' string values (§9); normalization is part of
// writing a key as a JSON string. So keys that only become equal under NFC
// are not SERE0022, and the output names the member twice.
#[test]
fn test_duplicate_names_are_checked_before_normalization() {
    let arg = "map{codepoints-to-string(233):1, concat('e', codepoints-to-string(769)):2}";
    let out = json(arg, "'normalization-form':'NFC'");
    assert_eq!(out.matches(ch(233)).count(), 2, "{out}");
    assert_eq!(
        error_of(&format!(
            "parse-json({}, map{{'duplicates':'reject'}})",
            json_expr(arg, "'normalization-form':'NFC'")
        )),
        ErrorValue::FOJS0003
    );
    let out = json(arg, "");
    assert_eq!(out.matches(ch(233)).count(), 1, "{out}");
}

#[test]
fn test_unsupported_normalization_form_is_sesu0011() {
    for form in ["fully-normalized", "nfc", "NFX", ""] {
        for arg in ["'a'", "1", "()"] {
            let params = format!("'normalization-form':'{form}'");
            assert_eq!(
                json_error(arg, &params),
                ErrorValue::SESU0011,
                "{form} {arg}"
            );
        }
    }
}

#[test]
fn test_empty_sequence_is_null() {
    assert_eq!(json("()", ""), "null");
    assert_eq!(json("[()]", ""), "[null]");
    assert_eq!(json("map{'a':()}", ""), r#"{"a":null}"#);
}

#[test]
fn test_sequences_and_functions_are_errors() {
    for arg in ["(1, 2)", "1 to 3", "[(1, 2)]", "map{'a':(1, 2)}"] {
        assert_eq!(json_error(arg, ""), ErrorValue::SERE0023, "{arg}");
    }
    for arg in ["abs#1", "function($x){$x}", "[abs#1]", "map{'a':abs#1}"] {
        assert_eq!(json_error(arg, ""), ErrorValue::SERE0021, "{arg}");
    }
}
