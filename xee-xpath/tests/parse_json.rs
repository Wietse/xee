//! fn:parse-json (F&O 3.1 §17.5.1): the cases QT3 does not cover.
//!
//! XPath string literals have no escapes, so a JSON escape such as `\u0000`
//! in these expressions is JSON text, and in an expected value it is the
//! literal six characters.

mod common;

use common::run;
use xee_xpath::error::ErrorValue;

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

// Keys are compared as the strings they become: after `fallback`
// replacement, two distinct escapes can give the same key. The duplicates
// policy applies to them rather than map construction's XQDY0137.
#[test]
fn test_keys_that_collide_after_fallback() {
    let text = r#"'{"\uFFFF":1,"\uFFFE":2}'"#;
    let replacement = "codepoints-to-string(65533)";
    // The defaults: escape false, use-first, U+FFFD fallback.
    assert_true(&format!(
        "let $m := parse-json({text}) return map:size($m) eq 1 and $m({replacement}) eq 1"
    ));
    assert_true(&format!(
        "let $m := parse-json({text}, map{{'duplicates':'use-first','escape':false()}}) \
         return map:size($m) eq 1 and $m({replacement}) eq 1"
    ));
    assert_true(&format!(
        "let $m := parse-json({text}, map{{'duplicates':'use-last','escape':false()}}) \
         return map:size($m) eq 1 and $m({replacement}) eq 2"
    ));
    assert_eq!(
        error_of(&format!(
            "parse-json({text}, map{{'duplicates':'reject','escape':false()}})"
        )),
        ErrorValue::FOJS0003
    );
    // A user fallback that maps both to the same string.
    assert_true(&format!(
        "let $m := parse-json({text}, map{{'duplicates':'use-last','fallback':function($s){{'x'}}}}) \
         return map:size($m) eq 1 and $m('x') eq 2"
    ));
    assert_eq!(
        error_of(&format!(
            "parse-json({text}, map{{'duplicates':'reject','fallback':function($s){{'x'}}}})"
        )),
        ErrorValue::FOJS0003
    );
    // Under escape=true the keys stay distinct.
    assert_true(&format!(
        r#"let $m := parse-json({text}, map{{'duplicates':'reject','escape':true()}})
           return map:size($m) eq 2 and $m('\uFFFF') eq 1 and $m('\uFFFE') eq 2"#
    ));
}

// Keys are duplicates when they are equal after expanding escapes
// (§17.5.1), however the escapes are spelled, also when a user fallback
// replaces them. The identity fallback returns the six-character escape it
// receives, so these keys are also equal as final strings.
#[test]
fn test_keys_equal_after_expanding_escapes_with_a_fallback() {
    let identity = "'fallback':function($s){$s}";
    for text in [
        r#"'{"\u000b":1,"\u000B":2}'"#,
        r#"'{"\u0008":1,"\b":2}'"#,
        r#"'{"\u000C":1,"\f":2}'"#,
        r#"'{"\udead":1,"\uDEAD":2}'"#,
        r#"'{"\uFFFF":1,"\uffff":2}'"#,
    ] {
        let expr = format!("parse-json({text}, map{{'duplicates':'reject',{identity}}})");
        assert_eq!(error_of(&expr), ErrorValue::FOJS0003, "{expr}");
        assert_true(&format!(
            "let $m := parse-json({text}, map{{{identity}}}) \
             return map:size($m) eq 1 and $m?* eq 1"
        ));
        assert_true(&format!(
            "let $m := parse-json({text}, map{{'duplicates':'use-last',{identity}}}) \
             return map:size($m) eq 1 and $m?* eq 2"
        ));
    }
}

#[test]
fn test_duplicates_in_document_order() {
    let text = r#"'{"a":1,"b":2,"a":3,"a":4}'"#;
    assert_true(&format!(
        "let $m := parse-json({text}) return map:size($m) eq 2 and $m('a') eq 1"
    ));
    assert_true(&format!(
        "parse-json({text}, map{{'duplicates':'use-first'}})('a') eq 1"
    ));
    assert_true(&format!(
        "parse-json({text}, map{{'duplicates':'use-last'}})('a') eq 4"
    ));
    assert_eq!(
        error_of(&format!("parse-json({text}, map{{'duplicates':'reject'}})")),
        ErrorValue::FOJS0003
    );
    // A key compares equal to the same key written with an escape.
    assert_eq!(
        error_of(
            r#"parse-json('{"a":1,"\u0061":2}', map{'duplicates':'reject','escape':false()})"#
        ),
        ErrorValue::FOJS0003
    );
    // Duplicates are per object: the same key in sibling or nested objects
    // is not a duplicate.
    assert_true(
        r#"let $a := parse-json('[{"a":1},{"a":2,"b":{"a":3}}]', map{'duplicates':'reject'})
           return $a(1)('a') eq 1 and $a(2)('a') eq 2 and $a(2)('b')('a') eq 3"#,
    );
    // The duplicate's value is still parsed: a grammar error in it counts.
    assert_eq!(
        error_of(r#"parse-json('{"a":1,"a":[1,]}')"#),
        ErrorValue::FOJS0001
    );
}

#[test]
fn test_leading_byte_order_mark_is_ignored() {
    assert_true("deep-equal(parse-json(codepoints-to-string(65279) || '[1]'), [1e0])");
    assert_true("parse-json(codepoints-to-string(65279) || ' \"x\" ', map{}) eq 'x'");
    // A byte order mark followed only by whitespace is not a JSON text.
    assert_eq!(
        error_of("parse-json(codepoints-to-string(65279) || ' ')"),
        ErrorValue::FOJS0001
    );
    assert_eq!(
        error_of("parse-json(codepoints-to-string(65279))"),
        ErrorValue::FOJS0001
    );
    // Only a leading one is ignored.
    assert_eq!(
        error_of("parse-json('[1]' || codepoints-to-string(65279))"),
        ErrorValue::FOJS0001
    );
    assert_eq!(
        error_of("parse-json(' ' || codepoints-to-string(65279) || '[1]')"),
        ErrorValue::FOJS0001
    );
}

// Numbers use the xs:string to xs:double cast.
#[test]
fn test_numbers_are_cast_to_double() {
    assert_true("parse-json('1') instance of xs:double");
    assert_true("parse-json('1e400') eq xs:double('INF')");
    assert_true("parse-json('-1e400') eq xs:double('-INF')");
    assert_true("parse-json('-0') eq 0 and 1 div parse-json('-0') eq xs:double('-INF')");
    assert_true("1 div parse-json('0') eq xs:double('INF')");
    assert_true("parse-json('-1.5E+2') eq -150");
    assert_true("parse-json('2.5e-1') eq 0.25");
    assert_true("parse-json('1e-400') eq 0");
    assert_true("parse-json('12345678901234567890') eq 12345678901234567890e0");
}

fn nested(open: &str, close: &str, depth: usize) -> String {
    format!("{}{}", open.repeat(depth), close.repeat(depth))
}

#[test]
fn test_depth_limit() {
    assert_true(&format!(
        "parse-json('{}') instance of array(*)",
        nested("[", "]", 512)
    ));
    assert_eq!(
        error_of(&format!("parse-json('{}')", nested("[", "]", 513))),
        ErrorValue::FOJS0001
    );
    let object = |depth| format!("{}1{}", r#"{"a":"#.repeat(depth), "}".repeat(depth));
    assert_true(&format!("parse-json('{}') instance of map(*)", object(512)));
    assert_eq!(
        error_of(&format!("parse-json('{}')", object(513))),
        ErrorValue::FOJS0001
    );
    // Far past the limit: an error, not a stack overflow. The text is built
    // at run time, as a long string literal would strain the XPath parser.
    assert_eq!(
        error_of("parse-json(string-join((1 to 100000) ! '['))"),
        ErrorValue::FOJS0001
    );
}

// Every kind of grammar error is FOJS0001.
#[test]
fn test_grammar_errors_are_fojs0001() {
    for text in [
        "''",
        "'['",
        "'[1 2]'",
        "'[01]'",
        "'tru'",
        r#"'"\x"'"#,
        r#"'"\u12"'"#,
        "'\"' || codepoints-to-string(9) || '\"'",
        "'1 2'",
        "'[1,]'",
        r#"'{"a"}'"#,
    ] {
        let expr = format!("parse-json({text})");
        assert_eq!(error_of(&expr), ErrorValue::FOJS0001, "{expr}");
        // liberal is accepted, but parsing stays strict.
        let expr = format!("parse-json({text}, map{{'liberal':true()}})");
        assert_eq!(error_of(&expr), ErrorValue::FOJS0001, "{expr}");
    }
}

// escape=true: the special characters are written as canonical escapes,
// everything else as itself.
#[test]
fn test_escape_true_is_canonical() {
    let escaped = |text: &str| format!("parse-json({text}, map{{'escape':true()}})");
    // The two-character escapes.
    assert_true(&format!(
        r#"{} eq '\b\f\n\r\t\\'"#,
        escaped(r#"'"\b\f\n\r\t\\"'"#)
    ));
    // The same characters written as \u escapes get the two-character form.
    assert_true(&format!(
        r#"{} eq '\b\f\n\r\t\\'"#,
        escaped(r#"'"\u0008\u000c\u000A\u000D\u0009\u005c"'"#)
    ));
    // Other controls get six characters, with upper-case hex.
    assert_true(&format!(
        r#"{} eq '\u0000\u0001\u001F\u001F'"#,
        escaped(r#"'"\u0000\u0001\u001f\u001F"'"#)
    ));
    // x7F to x9F, literal or escaped.
    assert_true(&format!(
        r#"{} eq '\u007F\u0085\u009F'"#,
        escaped("'\"' || codepoints-to-string((127, 133, 159)) || '\"'")
    ));
    assert_true(&format!(
        r#"{} eq 'a\u007F\u009Fb'"#,
        escaped(r#"'"a\u007f\u009fb"'"#)
    ));
    // A literal one in a string that also holds an escape.
    assert_true(&format!(
        r#"{} eq '\n\u007F\u0085'"#,
        escaped(r#"'"\n' || codepoints-to-string((127, 133)) || '"'"#)
    ));
    // xA0, just past the range, is not special.
    assert_true(&format!(
        "{} eq 'x' || codepoints-to-string(160)",
        escaped(r#"'"x\u00A0"'"#)
    ));
    // Non-XML characters and lone surrogates.
    assert_true(&format!(
        r#"{} eq '\uFFFF\uFFFE\uDEAD\uD800'"#,
        escaped(r#"'"\uffff\uFFFE\udead\uD800"'"#)
    ));
    // A valid surrogate pair is a character, not special.
    assert_true(&format!(
        "{} eq codepoints-to-string(119070)",
        escaped(r#"'"\uD834\uDD1E"'"#)
    ));
    // " and / are not special: their escapes come out as the characters.
    assert_true(&format!(r#"{} eq '"/'"#, escaped(r#"'"\"\/"'"#)));
    // Characters that are not special come out raw even if escaped.
    assert_true(&format!(
        "{} eq 'A' || codepoints-to-string(233)",
        escaped(r#"'"\u0041\u00e9"'"#)
    ));
    // A plain string with no special character is unchanged.
    assert_true(&format!(r#"{} eq 'abc'"#, escaped(r#"'"abc"'"#)));
    // Keys too.
    assert_true(&format!(
        r#"map:keys({}) eq '\n\u007F'"#,
        escaped(r#"'{"\u000a\u007f":1}'"#)
    ));
}

#[test]
fn test_escape_false_replaces_only_non_xml_characters() {
    // x7F to x9F and the controls allowed in XML are valid characters.
    assert_true(
        r#"parse-json('"\u007f\u0085\t\n\r\\"') eq codepoints-to-string((127, 133, 9, 10, 13, 92))"#,
    );
    assert_true(
        "parse-json('\"' || codepoints-to-string(127) || '\"') eq codepoints-to-string(127)",
    );
    assert_true(
        r#"parse-json('"\u0000\b\uFFFF\uFFFE\uDEAD"') eq codepoints-to-string((65533, 65533, 65533, 65533, 65533))"#,
    );
    // Each lone surrogate is replaced once; a valid pair is kept.
    assert_true(
        r#"parse-json('"\uD800\uD800\uD834\uDD1E"') eq codepoints-to-string((65533, 65533, 119070))"#,
    );
}

// A user fallback receives the canonical six-character escape with
// upper-case hex, once per non-XML character and once per lone surrogate,
// however the input spells it: also U+0008 and U+000C, which JSON can write
// as \b and \f.
#[test]
fn test_fallback_gets_the_canonical_escape() {
    let bracket = "map{'fallback':function($s){'[' || $s || ']'}}";
    assert_true(&format!(
        r#"parse-json('"a\u0000b\udead\bc"', {bracket}) eq 'a[\u0000]b[\uDEAD][\u0008]c'"#
    ));
    assert_true(&format!(
        r#"parse-json('"\u0008\u000c\f\u000b\uffff\ufffe"', {bracket})
           eq '[\u0008][\u000C][\u000C][\u000B][\uFFFF][\uFFFE]'"#
    ));
    assert_true(&format!(
        r#"parse-json('"\uD800\uD800\uD834\uDD1E"', {bracket})
           eq '[\uD800][\uD800]' || codepoints-to-string(119070)"#
    ));
    // Not called for a valid pair, nor for characters valid in XML.
    let fail = "map{'fallback':function($s){error(QName('', 'CALLED'))}}";
    assert_true(&format!(
        r#"parse-json('"\uD834\uDD1E\u0041\t"', {fail}) eq codepoints-to-string((119070, 65, 9))"#
    ));
    // Keys too.
    assert_true(&format!(
        r#"map:keys(parse-json('{{"\u0000":1}}', {bracket})) eq '[\u0000]'"#
    ));
    // A result of type xs:untypedAtomic is converted to xs:string.
    assert_true(
        r#"parse-json('"\u0000"', map{'fallback':function($s){xs:untypedAtomic('u')}}) eq 'u'"#,
    );
}

// A non-XML character can also appear unescaped in the JSON text: here
// U+FFFF and U+FFFE are written into the XPath string literal. It is
// handled as if it were escaped.
#[test]
fn test_unescaped_non_xml_characters() {
    let bracket = "'fallback':function($s){'[' || $s || ']'}";
    let replaced = "'a' || codepoints-to-string(65533) || 'b' || codepoints-to-string(65533)";
    // The second text ends with an escaped space, so it is a string with an
    // escape.
    for (text, space) in [
        ("'\"a\u{FFFF}b\u{FFFE}\"'", ""),
        ("'\"a\u{FFFF}b\u{FFFE}\\u0020\"'", " || ' '"),
    ] {
        assert_true(&format!("parse-json({text}) eq {replaced}{space}"));
        assert_true(&format!(
            "parse-json({text}, map{{'escape':false()}}) eq {replaced}{space}"
        ));
        assert_true(&format!(
            "parse-json({text}, map{{{bracket}}}) eq 'a[\\uFFFF]b[\\uFFFE]'{space}"
        ));
        assert_true(&format!(
            "parse-json({text}, map{{'escape':true()}}) eq 'a\\uFFFFb\\uFFFE'{space}"
        ));
    }
    // As a key, it is the same key as its escape.
    let text = "'{\"\u{FFFF}\":1,\"\\uffff\":2}'";
    for options in [
        "map{'duplicates':'reject'}",
        "map{'duplicates':'reject','escape':true()}",
        "map{'duplicates':'reject','fallback':function($s){$s}}",
    ] {
        let expr = format!("parse-json({text}, {options})");
        assert_eq!(error_of(&expr), ErrorValue::FOJS0003, "{expr}");
    }
    assert_true(&format!(
        "let $m := parse-json({text}, map{{'duplicates':'use-last',{bracket}}}) \
         return map:size($m) eq 1 and $m('[\\uFFFF]') eq 2"
    ));
}

// The escape default is false in every case: the one-argument form is the
// same as the two-argument form with an empty map (§17.5.1), and an empty
// map is the same as omitting the argument (§1.5). This overrides the
// option table's "Default: true", in line with w3c/qt3tests#65.
#[test]
fn test_escape_default_and_fallback() {
    // A non-XML character and an escaped backslash, as a string and as a
    // key.
    for (text, expected) in [
        (r#"'"\u0000"'"#, "codepoints-to-string(65533)"),
        (r#"'"a\\b"'"#, r"'a\b'"),
    ] {
        for options in [
            "",
            ", map{}",
            ", map{'duplicates':'use-first'}",
            ", map{'liberal':false()}",
        ] {
            assert_true(&format!("parse-json({text}{options}) eq {expected}"));
        }
    }
    for (text, expected) in [
        (r#"'{"\u0000":1}'"#, "codepoints-to-string(65533)"),
        (r#"'{"a\\b":1}'"#, r"'a\b'"),
    ] {
        for options in ["", ", map{}", ", map{'duplicates':'use-last'}"] {
            assert_true(&format!(
                "map:keys(parse-json({text}{options})) eq {expected}"
            ));
        }
    }
    // escape=true only when given.
    assert_true(r#"parse-json('"a\\b"', map{'escape':true()}) eq 'a\\b'"#);
    let text = r#"'"\u0000"'"#;
    assert_true(&format!(
        "parse-json({text}, map{{'fallback':function($s){{'x'}}}}) eq 'x'"
    ));
    assert_true(&format!(
        "parse-json({text}, map{{'fallback':function($s){{'x'}},'escape':false()}}) eq 'x'"
    ));
    assert_eq!(
        error_of(&format!(
            "parse-json({text}, map{{'fallback':function($s){{'x'}},'escape':true()}})"
        )),
        ErrorValue::FOJS0005
    );
    // Also when the input needs no fallback, and when there is no input.
    assert_eq!(
        error_of("parse-json('1', map{'fallback':function($s){'x'},'escape':true()})"),
        ErrorValue::FOJS0005
    );
    assert_eq!(
        error_of("parse-json((), map{'fallback':function($s){'x'},'escape':true()})"),
        ErrorValue::FOJS0005
    );
}

/// The six-character escape of `code`, upper- or lower-case hex.
fn unicode_escape(code: u32, upper: bool) -> String {
    let backslash = '\\';
    if upper {
        format!("{backslash}u{code:04X}")
    } else {
        format!("{backslash}u{code:04x}")
    }
}

// The §17.5.3 fallback pattern: a map keyed by the six-character escapes of
// the C0 controls, upper-case hex, with an error on a miss. Each of the 29
// C0 controls that are not valid XML characters hits it, however the input
// spells it. Tab, LF and CR are valid XML characters and never reach it.
#[test]
fn test_fallback_lookup_of_c0_controls() {
    let controls: Vec<u32> = (0..0x20)
        .filter(|c| !matches!(c, 0x09 | 0x0A | 0x0D))
        .collect();
    assert_eq!(controls.len(), 29);
    let c0chars: Vec<String> = controls
        .iter()
        .map(|&c| format!("'{}':'[{c}]'", unicode_escape(c, true)))
        .collect();
    let options = format!(
        "map{{'fallback':function($char as xs:string) as xs:string {{
            let $c0chars := map{{{}}}, $replacement := $c0chars($char)
            return if (exists($replacement)) then $replacement
                   else error(QName('', 'MISS'), $char)
        }}}}",
        c0chars.join(",")
    );
    let expected: String = controls.iter().map(|c| format!("[{c}]")).collect();
    for upper in [true, false] {
        let text: String = controls.iter().map(|&c| unicode_escape(c, upper)).collect();
        assert_true(&format!(
            "parse-json('\"{text}\"', {options}) eq '{expected}'"
        ));
        // Keys too.
        assert_true(&format!(
            "map:keys(parse-json('{{\"{text}\":1}}', {options})) eq '{expected}'"
        ));
    }
    // The two-character forms.
    assert_true(&format!(r#"parse-json('"\b\f"', {options}) eq '[8][12]'"#));
    // Tab, LF and CR, however written, are themselves.
    assert_true(&format!(
        r#"parse-json('"\t\n\r\u0009\u000a\u000D"', {options})
           eq codepoints-to-string((9, 10, 13, 9, 10, 13))"#
    ));
    // Any other character that reaches the fallback misses the map.
    for text in [r#"'"\uFFFF"'"#, r#"'"\udead"'"#] {
        let error = error_of(&format!("parse-json({text}, {options})"));
        assert_eq!(error.code(), "MISS", "{text}");
    }
}

// Duplicate keys are detected on the decoded key, not on what the fallback
// returns for it: a fallback that returns a different string on each call
// does not split equal keys into two members, however their escapes are
// spelled.
#[test]
fn test_duplicates_are_detected_on_the_decoded_key() {
    let fresh = "'fallback':function($s){generate-id(parse-xml('<a/>'))}";
    // The fallback does return a different string on each call.
    assert_true(&format!(
        r#"let $a := parse-json('["\u0000","\u0000"]', map{{{fresh}}})
           return $a(1) ne $a(2)"#
    ));
    for text in [
        r#"'{"\u0000":1,"\u0000":2}'"#,
        r#"'{"\u000b":1,"\u000B":2}'"#,
        r#"'{"\b":1,"\u0008":2}'"#,
    ] {
        let expr = format!("parse-json({text}, map{{'duplicates':'reject',{fresh}}})");
        assert_eq!(error_of(&expr), ErrorValue::FOJS0003, "{expr}");
        for (options, value) in [
            (format!("map{{{fresh}}}"), 1),
            (format!("map{{'duplicates':'use-first',{fresh}}}"), 1),
            (format!("map{{'duplicates':'use-last',{fresh}}}"), 2),
        ] {
            assert_true(&format!(
                "let $m := parse-json({text}, {options}) \
                 return map:size($m) eq 1 and $m?* eq {value}"
            ));
        }
    }
}

#[test]
fn test_fallback_type_errors() {
    let text = r#"'"\u0000"'"#;
    for fallback in [
        // Not a function.
        "'x'",
        // Not exactly one item.
        "()",
        "(upper-case#1, lower-case#1)",
        // The wrong arity.
        "substring#2",
        "function(){'x'}",
        // The result is not a single xs:string.
        "function($s){1}",
        "function($s){()}",
        "function($s){('a', 'b')}",
        // The argument is not accepted.
        "abs#1",
    ] {
        let expr = format!("parse-json({text}, map{{'fallback':{fallback}}})");
        assert_eq!(error_of(&expr), ErrorValue::XPTY0004, "{expr}");
    }
    // The function type is checked when the options are read, even if the
    // fallback is never called.
    for fallback in ["substring#2", "function(){'x'}"] {
        let expr = format!("parse-json('1', map{{'fallback':{fallback}}})");
        assert_eq!(error_of(&expr), ErrorValue::XPTY0004, "{expr}");
    }
    // An error the fallback raises propagates.
    let error = error_of(&format!(
        "parse-json({text}, map{{'fallback':function($s){{error(QName('', 'USER9999'))}}}})"
    ));
    assert_eq!(error.code(), "USER9999");
}

// Option parameter conventions: a value that cannot be converted to the
// option's type is XPTY0004; a converted value outside the permitted set is
// FOJS0005.
#[test]
fn test_option_errors() {
    for options in [
        "map{'liberal':'liberal'}",
        "map{'liberal':1}",
        "map{'escape':'yes'}",
        "map{'duplicates':1}",
        "map{'duplicates':('use-first', 'use-last')}",
        // Each option's type is exactly one item, so an empty sequence is not
        // the option left out.
        "map{'liberal':()}",
        "map{'escape':()}",
        "map{'duplicates':()}",
        "map{'escape':(), 'fallback':function($s){$s}}",
    ] {
        let expr = format!("parse-json('1', {options})");
        assert_eq!(error_of(&expr), ErrorValue::XPTY0004, "{expr}");
    }
    for options in [
        "map{'duplicates':'retain'}",
        "map{'duplicates':'USE-FIRST'}",
        "map{'duplicates':''}",
    ] {
        let expr = format!("parse-json('1', {options})");
        assert_eq!(error_of(&expr), ErrorValue::FOJS0005, "{expr}");
    }
    // Unknown options are ignored.
    assert_true("parse-json('1', map{'spec':'RFC4627', 'liberal':false()}) eq 1");
}

#[test]
fn test_null_is_the_empty_sequence() {
    assert_true("empty(parse-json(()))");
    assert_true("empty(parse-json((), map{}))");
    assert_true("empty(parse-json('null'))");
    assert_true("empty(parse-json(' null ', map{}))");
    assert_true("let $a := parse-json('[null, 1]') return array:size($a) eq 2 and empty($a(1))");
    assert_true(
        r#"let $m := parse-json('{"a":null}') return map:contains($m, 'a') and empty($m('a'))"#,
    );
}

#[test]
fn test_values() {
    assert_true(
        r#"deep-equal(parse-json('{"x":1, "y":[3,4,5], "z":{}, "t":true, "f":false, "s":"a"}'),
           map{'x':1e0, 'y':[3e0,4e0,5e0], 'z':map{}, 't':true(), 'f':false(), 's':'a'})"#,
    );
    assert_true("deep-equal(parse-json('[[], [[]], {}]'), [[], [[]], map{}])");
    assert_true("parse-json('true') instance of xs:boolean");
    assert_true(r#"parse-json('"x"') instance of xs:string"#);
}
