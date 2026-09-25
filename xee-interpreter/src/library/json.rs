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
    // Without an options map `escape` is false: see `ParseJsonParameters`.
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
/// it is the text as written. It is the canonical escape, the same text that
/// escape=true writes: `\b` and `\f` for U+0008 and U+000C, otherwise `\u`
/// and four upper-case hexadecimal digits. So it depends only on the decoded
/// character, and keys that are equal after expanding escapes, which
/// `duplicates` treats as duplicates, stay equal after `fallback` however
/// their escapes are spelled (U+000B written with lower- or upper-case hex,
/// U+0008 written as `\b` or as a six-character escape).
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
        // The `escape` default. The option table says true, but the spec's
        // examples and QT3 assume false (https://github.com/w3c/qt3tests/issues/65),
        // so it is false without an options map (`parse_json1`) and true
        // with one. A `fallback` without `escape` is applied, as in the
        // spec's own example (map{'fallback':function($s){'['||$s||']'}}),
        // so `escape` is then false too: it can only be true when explicit.
        let escape = escape.unwrap_or(fallback.is_none());
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
                let key = string_value(key, settings.escape, fallback)?;
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

/// An open object: the members so far, keyed by their final xs:string, and
/// the key of the member whose value is being read.
struct Object {
    members: HashMap<String, sequence::Sequence>,
    key: Option<String>,
}

impl Object {
    fn new() -> Self {
        Object {
            members: HashMap::new(),
            key: None,
        }
    }

    /// Starts a member. Keys are compared as the strings they become, that
    /// is after escapes are expanded and `fallback` is applied, or in escaped
    /// form under `escape` (§17.5.1), by codepoints. Keys equal after
    /// expanding escapes are equal here too, because `fallback` receives the
    /// canonical escape (see `Fallback`); two keys that only become equal
    /// through `fallback` are duplicates as well, rather than a map
    /// construction error.
    fn key(&mut self, key: String, duplicates: Duplicates) -> error::Result<()> {
        if duplicates == Duplicates::Reject && self.members.contains_key(&key) {
            return Err(error::Error::FOJS0003);
        }
        self.key = Some(key);
        Ok(())
    }

    /// Ends a member: `use-first` keeps the value already there, `use-last`
    /// replaces it (members arrive in document order).
    fn value(&mut self, value: sequence::Sequence, duplicates: Duplicates) -> error::Result<()> {
        let key = self.key.take().ok_or(error::Error::FOJS0001)?;
        match duplicates {
            Duplicates::UseLast => {
                self.members.insert(key, value);
            }
            Duplicates::UseFirst | Duplicates::Reject => {
                self.members.entry(key).or_insert(value);
            }
        }
        Ok(())
    }

    fn into_map(self) -> error::Result<function::Map> {
        function::Map::new(
            self.members
                .into_iter()
                .map(|(key, value)| (atomic::Atomic::from(key), value))
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
/// not. `fallback` receives its canonical escape (see `Fallback`).
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
        let mut canonical = String::new();
        push_canonical_escape(&mut canonical, c);
        out.push_str(&fallback(&canonical)?);
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

/// Appends the canonical escape for a special character: the two-character
/// form where JSON has one, `\uXXXX` otherwise. `"` and `/` are not special,
/// so their two-character forms never occur. It is also the argument
/// `fallback` receives for a character that is not valid in XML.
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
/// examples (`\uDEAD`, `\uFFFF`). Every special character and every lone
/// surrogate is in the Basic Multilingual Plane, so four digits suffice.
fn push_unicode_escape(out: &mut String, code: u32) {
    // Writing to a String cannot fail.
    let _ = write!(out, "\\u{code:04X}");
}

pub(crate) fn static_function_descriptions() -> Vec<StaticFunctionDescription> {
    vec![wrap_xpath_fn!(parse_json1), wrap_xpath_fn!(parse_json2)]
}
