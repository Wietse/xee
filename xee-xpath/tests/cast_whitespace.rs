mod common;

use common::{run, run_with_variables};
use xee_xpath::{context::Variables, Item, Sequence};

// Casting xs:string / xs:untypedAtomic to a type whose whiteSpace facet is
// `collapse` applies that facet before the lexical check (F&O 3.1, 19.2).
// XML whitespace is space, tab, LF and CR only; other Unicode spaces stay
// and make the value invalid.

fn assert_true(expr: &str) {
    let result = run(expr).unwrap_or_else(|e| panic!("{expr}: {e:?}"));
    assert!(
        result.effective_boolean_value().unwrap(),
        "{expr} should be true"
    );
}

// XML whitespace around the value: tab, LF, space before; space, CR, LF after
const XML_WS: &str = "codepoints-to-string((9, 10, 32))";
const XML_WS_END: &str = "codepoints-to-string((32, 13, 10))";

#[test]
fn test_collapse_before_numeric_and_boolean_casts() {
    for (value, target, expected) in [
        ("10.01", "xs:decimal", "10.01"),
        ("2", "xs:integer", "2"),
        ("-5", "xs:byte", "-5"),
        ("7", "xs:nonNegativeInteger", "7"),
        ("2.5", "xs:float", "xs:float('2.5')"),
        ("2.5", "xs:double", "2.5e0"),
        ("INF", "xs:double", "xs:double('INF')"),
        ("true", "xs:boolean", "true()"),
        ("0", "xs:boolean", "false()"),
    ] {
        for input in [
            format!("{XML_WS} || '{value}' || {XML_WS_END}"),
            format!("xs:untypedAtomic({XML_WS} || '{value}' || {XML_WS_END})"),
        ] {
            assert_true(&format!("({input}) cast as {target} eq {expected}"));
        }
    }
}

#[test]
fn test_non_xml_whitespace_is_not_collapsed() {
    // U+00A0 no-break space, U+3000 ideographic space
    for cp in [160, 12288] {
        for target in [
            "xs:decimal",
            "xs:integer",
            "xs:float",
            "xs:double",
            "xs:boolean",
        ] {
            let value = if target == "xs:boolean" { "true" } else { "1" };
            assert_true(&format!(
                "not((codepoints-to-string({cp}) || '{value}') castable as {target})"
            ));
        }
    }
}

#[test]
fn test_internal_whitespace_stays_invalid() {
    for (value, target) in [
        ("1 2", "xs:integer"),
        ("1 .5", "xs:decimal"),
        ("1 e3", "xs:double"),
        ("tr ue", "xs:boolean"),
    ] {
        assert_true(&format!("not(' {value} ' castable as {target})"));
    }
}

#[test]
fn test_form_feed_is_not_xml_whitespace() {
    // U+000C is not an XML character, so XPath cannot construct it
    // (codepoints-to-string raises FOCH0001), but a host can pass it in.
    for target in [
        "xs:decimal",
        "xs:integer",
        "xs:float",
        "xs:double",
        "xs:boolean",
    ] {
        let value = if target == "xs:boolean" { "true" } else { "1" };
        let item: Item = format!("\u{0C}{value}\u{0C}").as_str().into();
        let sequence: Sequence = item.into();
        let variables = Variables::from([(
            xot::xmlname::OwnedName::new("v".to_string(), "".to_string(), "".to_string()),
            sequence,
        )]);
        let expr = format!("not($v castable as {target})");
        let result = run_with_variables(&expr, variables).unwrap();
        assert!(
            result.effective_boolean_value().unwrap(),
            "{expr} should be true"
        );
    }
}
