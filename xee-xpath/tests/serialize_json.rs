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
    // Doubles keep every digit they need to read back.
    assert_eq!(json("1.0000000000000002e0", ""), "1.0000000000000002");
    assert_eq!(json("0.10000000000000002e0", ""), "0.10000000000000002");
    assert_eq!(json("123456.78901234567e0", ""), "123456.78901234567");
    // Every one reads back as the same value.
    for arg in [
        "xs:integer('-1000000000000000000000000000000')",
        "12345678901234567.89",
        "1.5e30",
        "-0e0",
        "1.0000000000000002e0",
        "0.10000000000000002e0",
        "123456.78901234567e0",
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
    // Each map has its own names: the same key in a sibling map or in a
    // nested map is no duplicate.
    for (arg, expected) in [
        ("[map{'a':1}, map{'a':2}]", r#"[{"a":1},{"a":2}]"#),
        ("map{'a':map{'a':1}}", r#"{"a":{"a":1}}"#),
        ("map{'a':[map{'a':1}]}", r#"{"a":[{"a":1}]}"#),
    ] {
        assert_eq!(json(arg, ""), expected, "{arg}");
        assert_eq!(
            json(arg, "'allow-duplicate-names':false()"),
            expected,
            "{arg}"
        );
    }
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
// Every name Xee accepts is used at least once.
#[test]
fn test_encoding_escapes_what_it_cannot_represent() {
    // x, U+00E9, U+00FF (the last Latin-1 character), U+0100, U+1D11E
    let text = "codepoints-to-string((120, 233, 255, 256, 119070))";
    let all = quoted(&format!("x{}{}{}{}", ch(233), ch(255), ch(256), ch(119070)));
    assert_eq!(json(text, ""), all);
    for encoding in [
        "utf-8", "UTF-8", "UTF-16", "UTF-16BE", "UTF-16LE", "utf-32", "UTF-32", "UTF-32BE",
        "UTF-32LE",
    ] {
        let params = format!("'encoding':'{encoding}'");
        assert_eq!(json(text, &params), all, "{encoding}");
    }
    let pair = format!("{}{}", esc("D834"), esc("DD1E"));
    let ascii = quoted(&format!(
        "x{}{}{}{pair}",
        esc("00E9"),
        esc("00FF"),
        esc("0100")
    ));
    for encoding in [
        "US-ASCII",
        "us-ascii",
        "ASCII",
        "ISO646-US",
        "ANSI_X3.4-1968",
    ] {
        let params = format!("'encoding':'{encoding}'");
        assert_eq!(json(text, &params), ascii, "{encoding}");
    }
    let latin1 = quoted(&format!("x{}{}{}{pair}", ch(233), ch(255), esc("0100")));
    for encoding in [
        "ISO-8859-1",
        "iso-8859-1",
        "ISO_8859-1",
        "ISO8859-1",
        "L1",
        "latin1",
        "LATIN1",
    ] {
        let params = format!("'encoding':'{encoding}'");
        assert_eq!(json(text, &params), latin1, "{encoding}");
    }
    // The Latin-1 bound on its own: U+00FF is written as itself under
    // ISO-8859-1 and escaped under US-ASCII.
    let y = "codepoints-to-string(255)";
    assert_eq!(
        json(y, "'encoding':'ISO-8859-1'"),
        quoted(&ch(255).to_string())
    );
    assert_eq!(json(y, "'encoding':'US-ASCII'"), quoted(&esc("00FF")));
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

// §3 only asks for printable ASCII (and SHOULD for a registered charset
// name), so these are valid values the JSON method does not support. The
// empty string holds no character outside #x21-#x7E; it names no charset,
// and is SESU0007 like any other name Xee does not know.
#[test]
fn test_unsupported_encoding_is_sesu0007() {
    for encoding in ["EBCDIC-US", "x-unknown", "", "!~"] {
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

// §3 defines allow-duplicate-names by the serialized JSON object, and §9
// normalizes a key before it is written. So keys that only become equal
// under the normalization form are SERE0022 as well, and the output never
// holds a member name twice, which parse-json would reject under
// duplicates: reject.
#[test]
fn test_duplicate_names_are_checked_after_normalization() {
    // U+00E9, and e followed by U+0301: equal under every normalization form
    let arg = "map{codepoints-to-string(233):1, concat('e', codepoints-to-string(769)):2}";
    for form in ["NFC", "NFD", "NFKC", "NFKD"] {
        for allow in ["", ",'allow-duplicate-names':false()"] {
            let params = format!("'normalization-form':'{form}'{allow}");
            assert_eq!(json_error(arg, &params), ErrorValue::SERE0022, "{params}");
        }
    }
    // Allowed, both members are written, the name U+00E9 twice.
    let out = json(
        arg,
        "'normalization-form':'NFC','allow-duplicate-names':true()",
    );
    let e = ch(233);
    assert!(
        out == format!(r#"{{"{e}":1,"{e}":2}}"#) || out == format!(r#"{{"{e}":2,"{e}":1}}"#),
        "{out}"
    );
    // Without normalization the names differ: no error, U+00E9 once, and
    // the output reads back under duplicates: reject.
    for params in ["", "'normalization-form':'none'"] {
        let out = json(arg, params);
        assert_eq!(out.matches(e).count(), 1, "{out}");
        assert_eq!(out.matches(ch(769)).count(), 1, "{out}");
        assert_true(&format!(
            "map:size(parse-json({}, map{{'duplicates':'reject'}})) eq 2",
            json_expr(arg, params)
        ));
    }
    // Keys with the same string value stay SERE0022 under every form.
    let same = "map{xs:QName(concat('e', codepoints-to-string(769))):1, \
                concat('e', codepoints-to-string(769)):2}";
    for form in ["none", "NFC", "NFD", "NFKC", "NFKD"] {
        let params = format!("'normalization-form':'{form}'");
        assert_eq!(json_error(same, &params), ErrorValue::SERE0022, "{form}");
    }
}

// Valid NMTOKENs (§3) that the JSON method does not support (§9.1.9).
#[test]
fn test_unsupported_normalization_form_is_sesu0011() {
    for form in ["fully-normalized", "nfc", "NFX", "1", "a:b.c-d"] {
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

// §3: normalization-form is an NMTOKEN and encoding a string of printable
// ASCII (#x21-#x7E); F&O 3.1 fn:serialize: a value that breaks these rules
// is SEPM0016. It is checked when the parameters are read, so it holds for
// every output method, whether or not the method uses the parameter.
#[test]
fn test_values_outside_the_parameter_domain_are_sepm0016() {
    let forms = [
        "''",
        "'a b'",
        "'a,b'",
        // an xs:NMTOKEN value holds no whitespace
        "' NFC'",
        "concat('NFC', codepoints-to-string(9))",
    ];
    let encodings = [
        "'a b'",
        "' utf-8'",
        "concat('utf-8', codepoints-to-string(9))",
        "concat('utf-8', codepoints-to-string(233))",
        // U+007F, just past the range
        "concat('utf-8', codepoints-to-string(127))",
    ];
    let params = forms
        .iter()
        .map(|form| format!("'normalization-form':{form}"))
        .chain(
            encodings
                .iter()
                .map(|encoding| format!("'encoding':{encoding}")),
        );
    for param in params {
        for method in [
            "'method':'json',",
            "'method':'xml',",
            "'method':'text',",
            "",
        ] {
            for arg in ["'a'", "1", "()"] {
                let expr = format!("serialize({arg}, map{{{method}{param}}})");
                assert_eq!(error_of(&expr), ErrorValue::SEPM0016, "{expr}");
            }
        }
    }
    // The ends of the encoding range, and an NMTOKEN with a character
    // outside ASCII, are inside the domains: the xml method, which applies
    // neither parameter, serializes.
    for param in [
        "'encoding':'!~'",
        "'normalization-form':concat('NF', codepoints-to-string(233))",
    ] {
        assert_true(&format!("serialize('a', map{{{param}}}) eq 'a'"));
    }
    assert_eq!(
        json_error(
            "'a'",
            "'normalization-form':concat('NF', codepoints-to-string(233))"
        ),
        ErrorValue::SESU0011
    );
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
