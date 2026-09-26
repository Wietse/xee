//! `fn:parse-json` (F&O 3.1 §17.5.1), built on the `xee-json` tokenizer.
//!
//! The tokenizer checks the RFC 8259 grammar and delivers events in document
//! order; this module turns them into XDM values and applies the options:
//! `duplicates`, `escape` and `fallback`, with numbers converted by the
//! xs:string to xs:double cast.

use std::fmt::Write as _;

use ahash::{HashMap, HashMapExt};
use xee_json::{BomPolicy, Decoded, Event, Options, Segment, Str, Tokenizer};
use xee_schema_type::Xs;
use xee_xpath_macros::xpath_fn;
use xee_xpath_type::ast;

use crate::{atomic, context, error, function, interpreter::Interpreter, sequence, wrap_xpath_fn};

use super::string::is_valid_xml_char;
use super::StaticFunctionDescription;

#[xpath_fn("fn:parse-json($json_text as xs:string?) as item()?")]
fn parse_json1(json_text: Option<&str>) -> error::Result<Option<sequence::Item>> {
    let Some(json_text) = json_text else {
        return Ok(None);
    };
    // The one-argument form is the two-argument form with an empty map
    // (§17.5.1), so every option has its default; `escape` defaults to
    // false, see `ParseJsonParameters::from_map`.
    let settings = Settings {
        duplicates: Duplicates::UseFirst,
        escape: false,
    };
    parse(json_text, &settings, &mut replacement_character)
}

#[xpath_fn("fn:parse-json($json_text as xs:string?, $options as map(*)) as item()?")]
fn parse_json2(
    context: &context::DynamicContext,
    interpreter: &mut Interpreter,
    json_text: Option<&str>,
    options: function::Map,
) -> error::Result<Option<sequence::Item>> {
    let parameters =
        ParseJsonParameters::from_map(&options, context.static_context(), interpreter)?;
    let Some(json_text) = json_text else {
        return Ok(None);
    };
    match parameters.fallback {
        Some(function) => {
            let mut fallback = |source: &str| call_fallback(interpreter, &function, source);
            parse(json_text, &parameters.settings, &mut fallback)
        }
        None => parse(json_text, &parameters.settings, &mut replacement_character),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Duplicates {
    Reject,
    UseFirst,
    UseLast,
}

/// The options that shape the result.
struct Settings {
    duplicates: Duplicates,
    escape: bool,
}

/// A function that replaces a character that is not a valid XML character,
/// or a lone surrogate: it receives a JSON escape sequence for it and
/// returns the replacement text.
///
/// Which characters reach it. The option table's escape=false row says every
/// character valid in XML is represented unescaped in the result and only
/// the others go to `fallback`, and the `fallback` row says it is called for
/// an escape sequence "that represents a character that is not valid" in
/// XML. The "User-supplied function" paragraph instead says it is called for
/// any special character as defined under `escape`, which would add tab, LF,
/// CR, x7F to x9F and the backslash. That reading contradicts the escape=false
/// row and the spec's own example, where `{"x":"\\", ...}` parsed with
/// map{'fallback':function($s){'['||$s||']'}} gives "x" a plain backslash.
/// So `fallback` gets exactly the characters that are not valid XML 1.0
/// characters, and each lone surrogate.
///
/// What it receives. §17.5.1 says the argument is always a two- or
/// six-character escape sequence that conforms to the JSON grammar, not that
/// it is the text as written. It is always the six-character form, `\u` and
/// four upper-case hexadecimal digits, also for U+0008 and U+000C, which
/// JSON can write as `\b` and `\f`. It depends only on the decoded
/// character, however the input spells it, and it is the form the §17.5.3
/// fallback example looks up (a map keyed by `'\u0000'` to `'\u001F'`), so
/// that pattern works for every character that reaches `fallback`. This is
/// not the text escape=true writes, which keeps the two-character forms
/// (see `push_canonical_escape`).
///
/// Whether two keys are duplicates does not depend on what `fallback`
/// returns for them: keys are compared after expanding escapes (see
/// `Object::key`).
type Fallback<'a> = dyn FnMut(&str) -> error::Result<String> + 'a;

/// The default `fallback`: U+FFFD REPLACEMENT CHARACTER.
fn replacement_character(_source: &str) -> error::Result<String> {
    Ok("\u{FFFD}".to_string())
}

struct ParseJsonParameters {
    settings: Settings,
    fallback: Option<function::Function>,
}

impl ParseJsonParameters {
    /// Reads the options under the option parameter conventions (F&O 3.1
    /// §1.5): a value that cannot be converted to the option's type is a type
    /// error (XPTY0004, or FORG0001 from a failed cast), and only a converted
    /// value outside the option's permitted values is FOJS0005. Every option
    /// type is exactly one item, so an entry whose value is the empty
    /// sequence is a type error, not the option left out.
    fn from_map(
        map: &function::Map,
        static_context: &context::StaticContext,
        interpreter: &Interpreter,
    ) -> error::Result<Self> {
        let c = sequence::OptionParameterConverter::new(map, static_context, interpreter.xot());

        // `liberal` is accepted and type-checked, but parsing stays strict:
        // §17.5.1 allows FOJS0001 for any deviation from the grammar even
        // when `liberal` is true.
        let _liberal: bool = c.one_with_default("liberal", Xs::Boolean, false)?;
        let duplicates: String =
            c.one_with_default("duplicates", Xs::String, "use-first".to_string())?;
        let duplicates = match duplicates.as_str() {
            "reject" => Duplicates::Reject,
            "use-first" => Duplicates::UseFirst,
            "use-last" => Duplicates::UseLast,
            _ => return Err(error::Error::FOJS0005),
        };
        let escape: Option<bool> = c.one("escape", Xs::Boolean)?;
        let fallback = match map.get(&atomic::Atomic::from("fallback")) {
            Some(value) => Some(fallback_function(value, interpreter)?),
            None => None,
        };
        // §17.5.1: FOJS0005 if `fallback` is supplied and `escape` is
        // present with the value true.
        if escape == Some(true) && fallback.is_some() {
            return Err(error::Error::FOJS0005);
        }
        // The `escape` default is false in every case: without an options
        // map (`parse_json1`), with an empty map, and with a map that has
        // other options, `fallback` among them. §17.5.1 says the
        // one-argument form is the same as the two-argument form with an
        // empty map, and §1.5 says an empty map has the same effect as
        // omitting the argument. Those two rules outweigh the option
        // table's "Default: true", and false is what the spec's examples
        // (among them the `fallback` example) and QT3 assume
        // (https://github.com/w3c/qt3tests/issues/65). So `escape` is true
        // only when it is given as true.
        let escape = escape.unwrap_or(false);
        Ok(Self {
            settings: Settings { duplicates, escape },
            fallback,
        })
    }
}

/// The `fallback` option, converted to `function(xs:string) as xs:string`:
/// exactly one function item of arity 1, otherwise XPTY0004. The argument
/// and the result are converted when the function is called.
fn fallback_function(
    value: &sequence::Sequence,
    interpreter: &Interpreter,
) -> error::Result<function::Function> {
    let function = value.clone().one()?.to_function()?;
    if interpreter.function_arity(&function) != 1 {
        return Err(error::Error::XPTY0004);
    }
    Ok(function)
}

/// Calls a user `fallback` with a JSON escape sequence; the result is
/// converted to a single xs:string by the function conversion rules. An
/// error the function raises (for example with `fn:error`) propagates.
fn call_fallback(
    interpreter: &mut Interpreter,
    function: &function::Function,
    source: &str,
) -> error::Result<String> {
    let argument: sequence::Sequence = atomic::Atomic::from(source).into();
    let result = interpreter.call_function_with_arguments(function, &[argument])?;
    let string_type = ast::SequenceType::Item(ast::Item {
        occurrence: ast::Occurrence::One,
        item_type: ast::ItemType::AtomicOrUnionType(Xs::String),
    });
    let runnable = interpreter.runnable();
    let result = result.sequence_type_matching_function_conversion(
        &string_type,
        runnable.static_context(),
        interpreter.typed_value_provider(),
        interpreter.xot(),
        &|function| runnable.program().function_info(function).signature(),
    )?;
    result.one()?.to_atomic()?.to_string()
}

/// Parses `text` into an XDM value.
///
/// The events are consumed in one loop with an explicit stack of open
/// containers, so nesting costs no native stack; the tokenizer bounds it at
/// its default depth. A leading byte order mark is ignored (§17.5.1), and
/// every tokenizer error is FOJS0001.
fn parse(
    text: &str,
    settings: &Settings,
    fallback: &mut Fallback<'_>,
) -> error::Result<Option<sequence::Item>> {
    let options = Options::default().with_bom_policy(BomPolicy::Ignore);
    let mut stack: Vec<Container> = Vec::new();
    let mut result: Option<sequence::Sequence> = None;
    for event in Tokenizer::with_options(text, options) {
        let event = event.map_err(|_| error::Error::FOJS0001)?;
        let value: sequence::Sequence = match event {
            Event::StartArray => {
                stack.push(Container::Array(Vec::new()));
                continue;
            }
            Event::StartObject => {
                stack.push(Container::Object(Object::new()));
                continue;
            }
            Event::Key(key) => {
                let key = object_key(key, settings.escape, fallback)?;
                match stack.last_mut() {
                    Some(Container::Object(object)) => object.key(key, settings.duplicates)?,
                    // The tokenizer delivers keys only inside objects.
                    _ => return Err(error::Error::FOJS0001),
                }
                continue;
            }
            Event::EndArray => match stack.pop() {
                Some(Container::Array(members)) => function::Array::new(members).into(),
                _ => return Err(error::Error::FOJS0001),
            },
            Event::EndObject => match stack.pop() {
                Some(Container::Object(object)) => object.into_map()?.into(),
                _ => return Err(error::Error::FOJS0001),
            },
            Event::String(s) => {
                atomic::Atomic::from(string_value(s, settings.escape, fallback)?).into()
            }
            Event::Number(number) => double(number)?.into(),
            Event::Bool(b) => atomic::Atomic::Boolean(b).into(),
            // null is the empty sequence, also as an array member or a map
            // value.
            Event::Null => sequence::Sequence::default(),
        };
        match stack.last_mut() {
            None => result = Some(value),
            Some(Container::Array(members)) => members.push(value),
            Some(Container::Object(object)) => object.value(value, settings.duplicates)?,
        }
    }
    // A text the tokenizer drained without an error holds exactly one value.
    match result {
        Some(value) if stack.is_empty() => value.option(),
        _ => Err(error::Error::FOJS0001),
    }
}

/// A JSON number, "converted to an xs:double value using the rules for
/// casting from xs:string to xs:double" (§17.5.1): an overflow gives ±INF and
/// `-0` keeps its sign. The tokenizer has checked the RFC 8259 grammar, which
/// is a subset of the xs:double lexical space.
fn double(number: &str) -> error::Result<atomic::Atomic> {
    atomic::Atomic::from(number).cast_to_double()
}

/// An open array or object.
enum Container {
    Array(Vec<sequence::Sequence>),
    Object(Object),
}

/// An object member's key: the xs:string it becomes, and, if `fallback`
/// was applied to any of its characters, the key as it is after expanding
/// escapes.
struct Key {
    string: String,
    decoded: Option<String>,
}

/// The key of an object member (see `Object::key`).
///
/// The decoded key is only needed when `fallback` was applied: it holds a
/// lone surrogate or a character that is not valid in XML, which the
/// xs:string cannot represent. It is written in the canonical escaped form
/// of escape=true, which represents every decoded key, lone surrogates
/// included, and is one-to-one because the backslash is escaped too. Under
/// escape=true `fallback` is never called, so the string is already that
/// form and there is no decoded key.
fn object_key(s: Str<'_>, escape: bool, fallback: &mut Fallback<'_>) -> error::Result<Key> {
    let mut replaced = false;
    let string = string_value(s, escape, &mut |source: &str| {
        replaced = true;
        fallback(source)
    })?;
    let decoded = if replaced {
        // escape=true never calls the function it is given.
        Some(string_value(s, true, &mut replacement_character)?)
    } else {
        None
    };
    Ok(Key { string, decoded })
}

/// An open object: the members so far, keyed by their final xs:string, and
/// the key of the member whose value is being read.
struct Object {
    members: HashMap<String, Member>,
    /// For each member whose key has a decoded form (see `Key`), that form
    /// and the member's final key.
    decoded: HashMap<String, String>,
    key: Option<Key>,
}

struct Member {
    value: sequence::Sequence,
    decoded: Option<String>,
}

impl Object {
    fn new() -> Self {
        Object {
            members: HashMap::new(),
            decoded: HashMap::new(),
            key: None,
        }
    }

    /// Starts a member.
    ///
    /// §17.5.1 compares keys by codepoints after expanding escapes, or in
    /// escaped form under `escape`. So a key is a duplicate of an earlier
    /// one when both are equal after expanding escapes, whatever `fallback`
    /// returns for them: two occurrences of `\u0000` are duplicates even if
    /// `fallback` returns a different string on each call. Keys that are
    /// equal after expanding escapes and need no `fallback` are equal as
    /// final strings, and a key that needs `fallback` is never equal after
    /// expanding escapes to one that does not, so the decoded form is
    /// compared only for keys that have one.
    ///
    /// A key is also a duplicate when its final string equals an earlier
    /// one's, although the two differ after expanding escapes (for example
    /// U+FFFF and U+FFFE, both replaced by U+FFFD): the `duplicates` policy
    /// applies to them too, rather than map construction's XQDY0137, which
    /// could not arise from anything in the JSON text.
    ///
    /// A key is compared with the members kept so far: under `use-first` an
    /// ignored member leaves no trace, and under `use-last` neither does a
    /// replaced one. This only matters when `fallback` gives equal decoded
    /// keys different strings.
    fn key(&mut self, key: Key, duplicates: Duplicates) -> error::Result<()> {
        if duplicates == Duplicates::Reject && self.is_duplicate(&key) {
            return Err(error::Error::FOJS0003);
        }
        self.key = Some(key);
        Ok(())
    }

    /// Ends a member: `use-first` keeps the member already there, `use-last`
    /// replaces it (members arrive in document order). A member that
    /// replaces another keeps its own final key, the result of its own
    /// `fallback` calls; it replaces every earlier member it duplicates.
    fn value(&mut self, value: sequence::Sequence, duplicates: Duplicates) -> error::Result<()> {
        let key = self.key.take().ok_or(error::Error::FOJS0001)?;
        match duplicates {
            Duplicates::UseLast => {
                self.remove_duplicates_of(&key);
                self.insert(key, value);
            }
            Duplicates::UseFirst | Duplicates::Reject => {
                if !self.is_duplicate(&key) {
                    self.insert(key, value);
                }
            }
        }
        Ok(())
    }

    fn is_duplicate(&self, key: &Key) -> bool {
        self.members.contains_key(&key.string)
            || key
                .decoded
                .as_ref()
                .is_some_and(|decoded| self.decoded.contains_key(decoded))
    }

    /// Removes the earlier members `key` duplicates: at most one with the
    /// same decoded key and one with the same final key, which are
    /// different members only if `fallback` gave the same decoded key two
    /// different strings.
    fn remove_duplicates_of(&mut self, key: &Key) {
        if let Some(decoded) = &key.decoded {
            if let Some(string) = self.decoded.remove(decoded) {
                self.members.remove(&string);
            }
        }
        if let Some(member) = self.members.remove(&key.string) {
            if let Some(decoded) = member.decoded {
                self.decoded.remove(&decoded);
            }
        }
    }

    /// Adds a member whose key duplicates no member there.
    fn insert(&mut self, key: Key, value: sequence::Sequence) {
        if let Some(decoded) = &key.decoded {
            self.decoded.insert(decoded.clone(), key.string.clone());
        }
        self.members.insert(
            key.string,
            Member {
                value,
                decoded: key.decoded,
            },
        );
    }

    fn into_map(self) -> error::Result<function::Map> {
        function::Map::new(
            self.members
                .into_iter()
                .map(|(key, member)| (atomic::Atomic::from(key), member.value))
                .collect(),
        )
    }
}

/// The xs:string for a JSON string or key.
///
/// With `escape` false, every valid XML character is represented as itself
/// and each character that is not (including each lone surrogate) is
/// replaced by `fallback`'s result for its escape sequence. With `escape`
/// true, the special characters are written as canonical JSON escapes and
/// every other character as itself, however it was written in the input.
fn string_value(s: Str<'_>, escape: bool, fallback: &mut Fallback<'_>) -> error::Result<String> {
    let mut out = String::new();
    match s {
        Str::Plain(text) => {
            if !text.chars().any(|c| needs_handling(c, escape)) {
                return Ok(text.to_string());
            }
            push_literal(&mut out, text, escape, fallback)?;
        }
        Str::Escaped(escaped) => {
            out.reserve(escaped.raw().len());
            for segment in escaped.segments() {
                match segment {
                    Segment::Literal(text) => push_literal(&mut out, text, escape, fallback)?,
                    Segment::Escape {
                        decoded: Decoded::Char(c),
                        ..
                    } => push_char(&mut out, c, escape, fallback)?,
                    Segment::Escape {
                        decoded: Decoded::LoneSurrogate(unit),
                        ..
                    } => {
                        let mut canonical = String::new();
                        push_unicode_escape(&mut canonical, u32::from(unit));
                        if escape {
                            out.push_str(&canonical);
                        } else {
                            out.push_str(&fallback(&canonical)?);
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Appends unescaped input text. It cannot hold a lone surrogate, but a
/// non-XML character such as U+FFFF can appear literally, and is handled
/// as if it were escaped.
fn push_literal(
    out: &mut String,
    text: &str,
    escape: bool,
    fallback: &mut Fallback<'_>,
) -> error::Result<()> {
    for c in text.chars() {
        push_char(out, c, escape, fallback)?;
    }
    Ok(())
}

/// Appends one decoded character, whether it was escaped in the input or
/// not. `fallback` receives its six-character escape (see `Fallback`).
fn push_char(
    out: &mut String,
    c: char,
    escape: bool,
    fallback: &mut Fallback<'_>,
) -> error::Result<()> {
    if !needs_handling(c, escape) {
        out.push(c);
    } else if escape {
        push_canonical_escape(out, c);
    } else {
        let mut source = String::new();
        push_unicode_escape(&mut source, u32::from(c));
        out.push_str(&fallback(&source)?);
    }
    Ok(())
}

/// Whether `c` cannot be copied to the result as it is: under `escape`, a
/// special character; otherwise a character that is not valid in XML 1.0.
fn needs_handling(c: char, escape: bool) -> bool {
    if escape {
        is_special(c)
    } else {
        !is_valid_xml_char(c)
    }
}

/// The special characters of the `escape` option (§17.5.1): x00 to x1F, x7F
/// to x9F, characters that are not valid in XML (the lone surrogates are
/// handled separately), and the backslash.
fn is_special(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}' | '\\') || !is_valid_xml_char(c)
}

/// Appends the canonical escape escape=true writes for a special character:
/// the two-character form where JSON has one, `\uXXXX` otherwise. `"` and
/// `/` are not special, so their two-character forms never occur. `fallback`
/// receives the six-character form instead (see `Fallback`).
fn push_canonical_escape(out: &mut String, c: char) {
    match c {
        '\u{8}' => out.push_str("\\b"),
        '\u{C}' => out.push_str("\\f"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        '\\' => out.push_str("\\\\"),
        _ => push_unicode_escape(out, u32::from(c)),
    }
}

/// Appends `\uXXXX` with upper-case hexadecimal digits, as in the spec's
/// examples (`\uDEAD`, `\uFFFF`, the §17.5.3 fallback example's
/// `\u001F`). Every special character and every lone surrogate is in the
/// Basic Multilingual Plane, so four digits suffice.
fn push_unicode_escape(out: &mut String, code: u32) {
    // Writing to a String cannot fail.
    let _ = write!(out, "\\u{code:04X}");
}

pub(crate) fn static_function_descriptions() -> Vec<StaticFunctionDescription> {
    vec![wrap_xpath_fn!(parse_json1), wrap_xpath_fn!(parse_json2)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses `text` with escape=false and a fallback that returns
    /// `results[n]` on its n-th call, and returns the result with the
    /// arguments the fallback received, in call order.
    fn parse_with(
        text: &str,
        duplicates: Duplicates,
        results: &[&str],
    ) -> (error::Result<Option<sequence::Item>>, Vec<String>) {
        let mut calls: Vec<String> = Vec::new();
        let settings = Settings {
            duplicates,
            escape: false,
        };
        let result = parse(text, &settings, &mut |source: &str| {
            let result = results.get(calls.len()).ok_or(error::Error::FOER0000)?;
            calls.push(source.to_string());
            Ok(result.to_string())
        });
        (result, calls)
    }

    /// The members of a map of strings, sorted by key.
    fn members(item: Option<sequence::Item>) -> Vec<(String, String)> {
        let map = item.expect("a map").to_map().expect("a map");
        let mut members: Vec<(String, String)> = map
            .entries()
            .map(|(key, value)| {
                let value = value.clone().one().expect("one item");
                (
                    key.to_string().expect("a string key"),
                    value
                        .to_atomic()
                        .expect("an atomic")
                        .to_string()
                        .expect("a string"),
                )
            })
            .collect();
        members.sort();
        members
    }

    // Two occurrences of the same escaped key are duplicates even when the
    // fallback gives them different strings. Under use-last the member kept
    // has the last occurrence's own final key.
    #[test]
    fn test_duplicates_by_decoded_key_with_a_nondeterministic_fallback() {
        let text = r#"{"\u0000":"a","\u0000":"b"}"#;
        let (result, calls) = parse_with(text, Duplicates::UseFirst, &["k1", "k2"]);
        assert_eq!(members(result.unwrap()), [("k1".into(), "a".into())]);
        assert_eq!(calls, [r"\u0000", r"\u0000"]);
        let (result, _) = parse_with(text, Duplicates::UseLast, &["k1", "k2"]);
        assert_eq!(members(result.unwrap()), [("k2".into(), "b".into())]);
        let (result, calls) = parse_with(text, Duplicates::Reject, &["k1", "k2"]);
        assert!(matches!(result, Err(error::Error::FOJS0003)), "{result:?}");
        assert_eq!(calls.len(), 2);
    }

    // The third key has the first one's decoded key and the second one's
    // final key, so it duplicates both.
    #[test]
    fn test_a_key_that_duplicates_two_members() {
        let text = r#"{"\u0000":"a","\u0001":"b","\u0000":"c"}"#;
        let results = ["x", "y", "y"];
        let (result, _) = parse_with(text, Duplicates::UseFirst, &results);
        assert_eq!(
            members(result.unwrap()),
            [("x".into(), "a".into()), ("y".into(), "b".into())]
        );
        let (result, _) = parse_with(text, Duplicates::UseLast, &results);
        assert_eq!(members(result.unwrap()), [("y".into(), "c".into())]);
        let (result, _) = parse_with(text, Duplicates::Reject, &results);
        assert!(matches!(result, Err(error::Error::FOJS0003)), "{result:?}");
    }

    // Keys are compared with the members kept so far. Under use-last the
    // third member replaces both the first and the second; the fourth has
    // the second one's decoded key, but the second is gone, so it is kept
    // beside the third: each decoded key keeps its last member.
    #[test]
    fn test_a_replaced_member_leaves_no_trace() {
        let text = r#"{"\u0000":"a","\u0001":"b","\u0000":"c","\u0001":"d"}"#;
        let results = ["x", "y", "y", "z"];
        let (result, _) = parse_with(text, Duplicates::UseLast, &results);
        assert_eq!(
            members(result.unwrap()),
            [("y".into(), "c".into()), ("z".into(), "d".into())]
        );
        let (result, _) = parse_with(text, Duplicates::UseFirst, &results);
        assert_eq!(
            members(result.unwrap()),
            [("x".into(), "a".into()), ("y".into(), "b".into())]
        );
    }

    // The fallback is called for every key and every string value in
    // document order, those of an ignored duplicate included.
    #[test]
    fn test_fallback_calls_include_ignored_duplicates() {
        let text = r#"{"\u0000":"\u0001","\u0000":"\u0002"}"#;
        let (result, calls) = parse_with(text, Duplicates::UseFirst, &["k", "a", "k", "b"]);
        assert_eq!(members(result.unwrap()), [("k".into(), "a".into())]);
        assert_eq!(calls, [r"\u0000", r"\u0001", r"\u0000", r"\u0002"]);
    }
}
