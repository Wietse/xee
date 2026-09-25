use std::borrow::Cow;

use ahash::{HashMap, HashSet};
use icu::normalizer::{ComposingNormalizer, DecomposingNormalizer};
use rust_decimal::Decimal;
use xee_json::{EscapeProfile, Layout, WriteError, Writer};
use xot::{xmlname::OwnedName, Xot};

use xee_schema_type::Xs;

use crate::{
    atomic, context, error,
    function::{self, Map},
};

use super::{
    core::Sequence,
    item::Item,
    opc::{OptionParameterConverter, QNameOrString},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializationParameters {
    pub allow_duplicate_names: bool,
    pub byte_order_mark: bool,
    pub cdata_section_elements: Vec<OwnedName>,
    pub doctype_public: Option<String>,
    pub doctype_system: Option<String>,
    pub encoding: String,
    pub escape_uri_attributes: bool,
    pub html_version: Decimal,
    pub include_content_type: bool,
    pub indent: bool,
    pub item_separator: String,
    pub json_node_output_method: QNameOrString,
    pub media_type: Option<String>,
    pub method: QNameOrString,
    pub normalization_form: Option<String>,
    pub omit_xml_declaration: bool,
    pub standalone: Option<bool>,
    pub suppress_indentation: Vec<OwnedName>,
    pub undeclare_prefixes: bool,
    pub use_character_maps: HashMap<char, String>,
    pub version: String,
}

impl SerializationParameters {
    // default values are as used in XSLT 3.0
    pub fn new() -> Self {
        Self {
            allow_duplicate_names: false,
            byte_order_mark: false,
            cdata_section_elements: Vec::new(),
            doctype_public: None,
            doctype_system: None,
            encoding: "utf-8".to_string(),
            escape_uri_attributes: true,
            html_version: Decimal::from_str_exact("5.0").unwrap(),
            include_content_type: true,
            indent: false,
            item_separator: " ".to_string(),
            json_node_output_method: QNameOrString::String("xml".to_string()),
            media_type: Some("text/xml".to_string()),
            method: QNameOrString::String("xml".to_string()),
            normalization_form: None,
            omit_xml_declaration: false,
            standalone: None,
            suppress_indentation: Vec::new(),
            undeclare_prefixes: false,
            use_character_maps: HashMap::default(),
            version: "1.0".to_string(),
        }
    }

    pub(crate) fn from_map(
        map: Map,
        static_context: &context::StaticContext,
        xot: &Xot,
    ) -> error::Result<Self> {
        let c = OptionParameterConverter::new(&map, static_context, xot);
        let allow_duplicate_names =
            c.option_with_default("allow-duplicate-names", Xs::Boolean, false)?;

        let byte_order_mark = c.option_with_default("byte-order-mark", Xs::Boolean, false)?;

        let cdata_section_elements = c.many("cdata-section-elements", Xs::QName)?;

        let doctype_public = c.option("doctype-public", Xs::String)?;

        let doctype_system = c.option("doctype-system", Xs::String)?;

        let encoding = c.option_with_default("encoding", Xs::String, "utf-8".to_string())?;

        let escape_uri_attributes =
            c.option_with_default("escape-uri-attributes", Xs::Boolean, true)?;

        let html_version = c.option_with_default(
            "html-version",
            Xs::Decimal,
            Decimal::from_str_exact("5.0").unwrap(),
        )?;

        let include_content_type =
            c.option_with_default("include-content-type", Xs::Boolean, true)?;

        let indent = c.option_with_default("indent", Xs::Boolean, false)?;

        let item_separator =
            c.option_with_default("item-separator", Xs::String, " ".to_string())?;

        let json_node_output_method = c.qname_or_string(
            "json-node-output-method",
            QNameOrString::String("xml".to_string()),
        )?;

        let media_type = c.option("media-type", Xs::String)?;

        let method = c.qname_or_string("method", QNameOrString::String("xml".to_string()))?;

        // Serialization 3.1 §3: a no-namespace value outside these names is
        // invalid (SEPM0016) whether or not the parameter ends up being used.
        check_method_domain(
            &method,
            &["xml", "xhtml", "html", "text", "json", "adaptive"],
        )?;
        check_method_domain(&json_node_output_method, &["xml", "xhtml", "html", "text"])?;

        let normalization_form = c.option("normalization-form", Xs::String)?;

        let omit_xml_declaration =
            c.option_with_default("omit-xml-declaration", Xs::Boolean, true)?;

        let standalone = c.option("standalone", Xs::Boolean)?;

        let suppress_indentation = c.many("suppress-indentation", Xs::QName)?;

        let undeclare_prefixes = c.option_with_default("undeclare-prefixes", Xs::Boolean, false)?;

        // TODO: use-character-maps (Serialization 3.1 §11) is not read, so
        // no output method applies a character map. The xml and html
        // methods write through xot, which has no hook for one.

        let version = c.option_with_default("version", Xs::String, "1.0".to_string())?;

        Ok(Self {
            allow_duplicate_names,
            byte_order_mark,
            cdata_section_elements,
            doctype_public,
            doctype_system,
            encoding,
            escape_uri_attributes,
            html_version,
            include_content_type,
            indent,
            item_separator,
            json_node_output_method,
            media_type,
            method,
            normalization_form,
            omit_xml_declaration,
            standalone,
            suppress_indentation,
            undeclare_prefixes,
            use_character_maps: HashMap::default(),
            version,
        })
    }

    pub(crate) fn xml_in_json_serialization(method: &QNameOrString) -> Self {
        Self {
            // use the method given
            method: method.clone(),
            // the only thing set according to the specification
            omit_xml_declaration: true,
            // keep this around just in case, though I don't think we
            // can end up in json output from XML output
            json_node_output_method: method.clone(),
            allow_duplicate_names: false,
            byte_order_mark: false,
            cdata_section_elements: Vec::new(),
            doctype_public: None,
            doctype_system: None,
            encoding: "utf-8".to_string(),
            escape_uri_attributes: false,
            html_version: Decimal::from_str_exact("5.0").unwrap(),
            include_content_type: false,
            indent: false,
            item_separator: " ".to_string(),
            media_type: None,
            normalization_form: None,
            standalone: None,
            suppress_indentation: Vec::new(),
            undeclare_prefixes: false,
            use_character_maps: HashMap::default(),
            version: "1.0".to_string(),
        }
    }
}

impl Default for SerializationParameters {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn serialize_sequence(
    arg: &Sequence,
    parameters: SerializationParameters,
    xot: &mut Xot,
) -> error::Result<String> {
    match parameters.method.local_name() {
        Some("xml") => serialize_xml(arg, parameters, xot),
        Some("html") => serialize_html(arg, parameters, xot),
        Some("text") => serialize_text(arg, parameters, xot),
        Some("json") => serialize_json(arg, parameters, xot),
        Some(method @ ("xhtml" | "adaptive")) => Err(error::Error::Unsupported(format!(
            "the {method} output method is not supported"
        ))),
        // a QName in a namespace names an implementation-defined method;
        // Xee defines none
        None => Err(error::Error::Unsupported(
            "implementation-defined output methods are not supported".to_string(),
        )),
        Some(_) => Err(error::Error::SEPM0016),
    }
}

fn check_method_domain(value: &QNameOrString, names: &[&str]) -> error::Result<()> {
    match value.local_name() {
        Some(name) if !names.contains(&name) => Err(error::Error::SEPM0016),
        _ => Ok(()),
    }
}

fn serialize_xml(
    arg: &Sequence,
    parameters: SerializationParameters,
    xot: &mut Xot,
) -> Result<String, error::Error> {
    let node = arg.normalize(&parameters.item_separator, xot)?;
    let indentation = xot_indentation(&parameters, xot);
    let cdata_section_elements = xot_names(&parameters.cdata_section_elements, xot);
    let declaration = if !parameters.omit_xml_declaration {
        Some(xot::output::xml::Declaration {
            encoding: Some(parameters.encoding.to_string()),
            standalone: parameters.standalone,
        })
    } else {
        None
    };
    let doctype = match (parameters.doctype_public, parameters.doctype_system) {
        (Some(public), Some(system)) => Some(xot::output::xml::DocType::Public { public, system }),
        (None, Some(system)) => Some(xot::output::xml::DocType::System { system }),
        // TODO: this should really not happen?
        (Some(public), None) => Some(xot::output::xml::DocType::Public {
            public,
            system: "".to_string(),
        }),
        (None, None) => None,
    };
    let output_parameters = xot::output::xml::Parameters {
        indentation,
        cdata_section_elements,
        declaration,
        doctype,
        ..Default::default()
    };

    Ok(xot.serialize_xml_string(output_parameters, node)?)
}

fn serialize_html(
    arg: &Sequence,
    parameters: SerializationParameters,
    xot: &mut Xot,
) -> Result<String, error::Error> {
    let node = arg.normalize(&parameters.item_separator, xot)?;
    // TODO: no check yet for html version rejecting versions that aren't 5
    let cdata_section_elements = xot_names(&parameters.cdata_section_elements, xot);
    let indentation = xot_indentation(&parameters, xot);
    // xot writes `<!DOCTYPE html>` for every node. Serialization 3.1 §7.4.6
    // asks for it only when the first element is named html (in any case)
    // with nothing but whitespace text before it, and the doctype
    // parameters are absent.
    let html5 = xot.html5();
    let output_parameters = xot::output::html5::Parameters {
        indentation,
        cdata_section_elements,
    };
    Ok(html5.serialize_string(output_parameters, node)?)
}

// Serialization 3.1 §8: the string value of the document node that sequence
// normalization produces, without escaping.
fn serialize_text(
    arg: &Sequence,
    parameters: SerializationParameters,
    xot: &mut Xot,
) -> Result<String, error::Error> {
    let node = arg.normalize(&parameters.item_separator, xot)?;
    Ok(xot.string_value(node))
}

// Serialization 3.1 §9, the JSON output method, written through the
// xee-json writer with the §9 escaping profile.
//
// - Numbers are written as their canonical xs:string form (F&O 3.1
//   §19.1.2), which keeps xs:integer and xs:decimal values exact; §9 allows
//   any RFC 8259 form.
// - Map members are written in the map's order. Keys with the same string
//   value are SERE0022 unless allow-duplicate-names is true, in which case
//   both members are written.
// - `indent` selects the writer's indented layout.
// - `encoding` gives the writer its "is encodable" predicate (§9.1.3); an
//   encoding Xee does not know is SESU0007. Since every character can be
//   escaped, SERE0008 cannot arise.
// - `normalization-form` is applied to each string, keys and node output
//   included, before it is escaped (§9, §9.1.9).
// - Not implemented: character maps (§11). `use-character-maps` is not read
//   from the parameter map (see `from_map`); the writer's verbatim string
//   parts are there for it.
// - `byte-order-mark` does not apply: fn:serialize returns a string, not
//   octets.
//
// The serializer recurses once per nested array or map. The value is an
// XDM value the program built, and its nesting is that of the program's
// data; the writer itself keeps no recursion.
fn serialize_json(
    arg: &Sequence,
    parameters: SerializationParameters,
    xot: &mut Xot,
) -> Result<String, error::Error> {
    let encodable = json_encodable(&parameters.encoding)?;
    let normalization = Normalization::new(parameters.normalization_form.as_deref())?;
    let layout = if parameters.indent {
        Layout::Indented
    } else {
        Layout::Compact
    };
    let writer = Writer::new(layout, EscapeProfile::Serialization);
    let writer = match encodable {
        Some(encodable) => writer.with_encodable(encodable),
        None => writer,
    };
    let mut serializer = JsonSerializer {
        writer,
        parameters: &parameters,
        normalization,
    };
    serializer.sequence(arg, xot)?;
    serializer.writer.finish().map_err(write_error)
}

/// Whether an output encoding can represent a character.
type Encodable = fn(char) -> bool;

/// The "is encodable" predicate for an output encoding (Serialization 3.1
/// §9.1.3), or `None` when the encoding can represent every character.
/// UTF-8 and UTF-16 are required; UTF-32, US-ASCII and ISO-8859-1 are also
/// supported. Names are compared without regard to case, as charset names
/// are. Any other encoding is SESU0007.
fn json_encodable(encoding: &str) -> error::Result<Option<Encodable>> {
    match encoding.to_ascii_uppercase().as_str() {
        "UTF-8" | "UTF-16" | "UTF-16BE" | "UTF-16LE" | "UTF-32" | "UTF-32BE" | "UTF-32LE" => {
            Ok(None)
        }
        "US-ASCII" | "ASCII" | "ISO646-US" | "ANSI_X3.4-1968" => Ok(Some(is_ascii)),
        "ISO-8859-1" | "ISO_8859-1" | "ISO8859-1" | "LATIN1" | "L1" => Ok(Some(is_latin1)),
        _ => Err(error::Error::SESU0007),
    }
}

fn is_ascii(c: char) -> bool {
    c.is_ascii()
}

fn is_latin1(c: char) -> bool {
    u32::from(c) <= 0xFF
}

/// The `normalization-form` parameter for the JSON output method
/// (Serialization 3.1 §9.1.9). NFC and none are required; NFD, NFKC and
/// NFKD are also supported. The values are the names §3 enumerates, as
/// written; `fully-normalized` and any other value are SESU0011.
enum Normalization {
    None,
    Composing(ComposingNormalizer),
    Decomposing(DecomposingNormalizer),
}

impl Normalization {
    fn new(form: Option<&str>) -> error::Result<Self> {
        match form {
            None | Some("none") => Ok(Normalization::None),
            Some("NFC") => Ok(Normalization::Composing(ComposingNormalizer::new_nfc())),
            Some("NFKC") => Ok(Normalization::Composing(ComposingNormalizer::new_nfkc())),
            Some("NFD") => Ok(Normalization::Decomposing(DecomposingNormalizer::new_nfd())),
            Some("NFKD") => Ok(Normalization::Decomposing(DecomposingNormalizer::new_nfkd())),
            Some(_) => Err(error::Error::SESU0011),
        }
    }

    fn apply<'s>(&self, s: &'s str) -> Cow<'s, str> {
        match self {
            Normalization::None => Cow::Borrowed(s),
            Normalization::Composing(normalizer) => Cow::Owned(normalizer.normalize(s)),
            Normalization::Decomposing(normalizer) => Cow::Owned(normalizer.normalize(s)),
        }
    }
}

/// A refused writer call. The serializer makes only calls that fit the
/// JSON grammar at that point, and every number text it passes is a
/// canonical xs:float, xs:double, xs:decimal or xs:integer string after the
/// INF and NaN check, which is always an RFC 8259 number; so none of these
/// is reached. They are errors rather than panics all the same: a number
/// the writer refuses cannot be represented in the JSON grammar (SERE0020),
/// and any other refusal is FOER0000.
fn write_error(error: WriteError) -> error::Error {
    match error {
        WriteError::InvalidNumber => error::Error::SERE0020,
        _ => error::Error::FOER0000,
    }
}

struct JsonSerializer<'p> {
    writer: Writer<'static>,
    parameters: &'p SerializationParameters,
    normalization: Normalization,
}

impl JsonSerializer<'_> {
    fn sequence(&mut self, arg: &Sequence, xot: &mut Xot) -> error::Result<()> {
        match arg {
            Sequence::One(item) => self.item(item.item(), xot),
            Sequence::Empty(_) => self.writer.null().map_err(write_error),
            Sequence::Many(_) | Sequence::Range(_) => Err(error::Error::SERE0023),
        }
    }

    fn item(&mut self, item: &Item, xot: &mut Xot) -> error::Result<()> {
        match item {
            Item::Atomic(atomic) => self.atomic(atomic),
            Item::Node(node) => self.node(*node, xot),
            Item::Function(function) => self.function(function, xot),
        }
    }

    fn atomic(&mut self, atomic: &atomic::Atomic) -> error::Result<()> {
        match atomic {
            atomic::Atomic::Float(float) if !float.into_inner().is_finite() => {
                Err(error::Error::SERE0020)
            }
            atomic::Atomic::Double(double) if !double.into_inner().is_finite() => {
                Err(error::Error::SERE0020)
            }
            atomic::Atomic::Float(_)
            | atomic::Atomic::Double(_)
            | atomic::Atomic::Decimal(_)
            | atomic::Atomic::Integer(..) => self
                .writer
                .number(&atomic.string_value())
                .map_err(write_error),
            atomic::Atomic::Boolean(b) => self.writer.bool(*b).map_err(write_error),
            _ => self.string(&atomic.string_value()),
        }
    }

    fn string(&mut self, s: &str) -> error::Result<()> {
        let s = self.normalization.apply(s);
        self.writer.string(&s).map_err(write_error)
    }

    fn key(&mut self, s: &str) -> error::Result<()> {
        let s = self.normalization.apply(s);
        self.writer.key(&s).map_err(write_error)
    }

    fn node(&mut self, node: xot::Node, xot: &mut Xot) -> error::Result<()> {
        // from_map already rejects values outside the parameter's domain;
        // this guards hand-built parameters, where `json` would recurse.
        if let Some("json" | "adaptive") = self.parameters.json_node_output_method.local_name() {
            return Err(error::Error::SEPM0016);
        }
        let node_parameters = SerializationParameters::xml_in_json_serialization(
            &self.parameters.json_node_output_method,
        );
        let sequence: Sequence = vec![node].into();
        let s = serialize_sequence(&sequence, node_parameters, xot)?;
        self.string(&s)
    }

    fn function(&mut self, function: &function::Function, xot: &mut Xot) -> error::Result<()> {
        match function {
            function::Function::Array(array) => self.array(array, xot),
            function::Function::Map(map) => self.map(map, xot),
            _ => Err(error::Error::SERE0021),
        }
    }

    fn array(&mut self, array: &function::Array, xot: &mut Xot) -> error::Result<()> {
        self.writer.start_array().map_err(write_error)?;
        for member in array.iter() {
            self.sequence(member, xot)?;
        }
        self.writer.end_array().map_err(write_error)
    }

    fn map(&mut self, map: &function::Map, xot: &mut Xot) -> error::Result<()> {
        self.writer.start_object().map_err(write_error)?;
        // §9: keys are compared by their string value, before normalization.
        let mut names = HashSet::default();
        for (key, value) in map.entries() {
            let name = key.string_value();
            if !self.parameters.allow_duplicate_names && !names.insert(name.clone()) {
                return Err(error::Error::SERE0022);
            }
            self.key(&name)?;
            self.sequence(value, xot)?;
        }
        self.writer.end_object().map_err(write_error)
    }
}

fn xot_indentation(
    parameters: &SerializationParameters,
    xot: &mut Xot,
) -> Option<xot::output::Indentation> {
    if !parameters.indent {
        return None;
    }
    let suppress = xot_names(&parameters.suppress_indentation, xot);
    Some(xot::output::Indentation { suppress })
}

fn xot_names(names: &[xot::xmlname::OwnedName], xot: &mut Xot) -> Vec<xot::NameId> {
    names
        .iter()
        .map(|owned_name| owned_name.to_ref(xot).name_id())
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{atomic, sequence};

    use super::*;

    #[test]
    fn test_allow_duplicate_names_true() {
        let map = Map::new(vec![(
            "allow-duplicate-names".to_string().into(),
            sequence::Sequence::from(vec![atomic::Atomic::Boolean(true)]),
        )])
        .unwrap();
        let static_context = context::StaticContext::default();
        let xot = Xot::new();
        let params = SerializationParameters::from_map(map, &static_context, &xot).unwrap();
        assert!(params.allow_duplicate_names);
    }

    #[test]
    fn test_allow_duplicate_names_false() {
        let map = Map::new(vec![(
            "allow-duplicate-names".to_string().into(),
            sequence::Sequence::from(vec![atomic::Atomic::Boolean(false)]),
        )])
        .unwrap();
        let static_context = context::StaticContext::default();
        let xot = Xot::new();
        let params = SerializationParameters::from_map(map, &static_context, &xot).unwrap();
        assert!(!params.allow_duplicate_names);
    }

    #[test]
    fn test_allow_duplicate_names_default_empty_sequence() {
        let map = Map::new(vec![(
            "allow-duplicate-names".to_string().into(),
            sequence::Sequence::default(),
        )])
        .unwrap();
        let static_context = context::StaticContext::default();
        let xot = Xot::new();
        let params = SerializationParameters::from_map(map, &static_context, &xot).unwrap();
        assert!(!params.allow_duplicate_names);
    }

    #[test]
    fn test_allow_duplicate_names_missing() {
        let map = Map::new(vec![]).unwrap();
        let static_context = context::StaticContext::default();
        let xot = Xot::new();
        let params = SerializationParameters::from_map(map, &static_context, &xot).unwrap();
        assert!(!params.allow_duplicate_names);
    }

    #[test]
    fn test_cdata_section_elements() {
        let html = OwnedName::new("html".to_string(), "".to_string(), "".to_string());
        let script = OwnedName::new("script".to_string(), "".to_string(), "".to_string());
        let map = Map::new(vec![(
            "cdata-section-elements".to_string().into(),
            sequence::Sequence::from(vec![
                atomic::Atomic::QName(html.clone().into()),
                atomic::Atomic::QName(script.clone().into()),
            ]),
        )])
        .unwrap();
        let static_context = context::StaticContext::default();
        let xot = Xot::new();
        let params = SerializationParameters::from_map(map, &static_context, &xot).unwrap();
        assert_eq!(params.cdata_section_elements.len(), 2);
        assert_eq!(params.cdata_section_elements[0], html);
        assert_eq!(params.cdata_section_elements[1], script);
    }

    #[test]
    fn test_qname_or_string_string() {
        let text: atomic::Atomic = "text".to_string().into();
        let map = Map::new(vec![(
            "json-node-output-method".to_string().into(),
            sequence::Sequence::from(vec![text]),
        )])
        .unwrap();
        let static_context = context::StaticContext::default();
        let xot = Xot::new();
        let params = SerializationParameters::from_map(map, &static_context, &xot).unwrap();
        assert_eq!(
            params.json_node_output_method,
            QNameOrString::String("text".to_string())
        );
    }

    #[test]
    fn test_qname_or_string_qname() {
        let owned_name = OwnedName::new("text".to_string(), "".to_string(), "".to_string());
        let text: atomic::Atomic = owned_name.clone().into();
        let map = Map::new(vec![(
            "json-node-output-method".to_string().into(),
            sequence::Sequence::from(vec![text]),
        )])
        .unwrap();
        let static_context = context::StaticContext::default();
        let xot = Xot::new();
        let params = SerializationParameters::from_map(map, &static_context, &xot).unwrap();
        assert_eq!(
            params.json_node_output_method,
            QNameOrString::QName(owned_name)
        );
    }

    #[test]
    fn test_qname_or_string_default_empty_sequence() {
        let map = Map::new(vec![(
            "json-node-output-method".to_string().into(),
            sequence::Sequence::default(),
        )])
        .unwrap();
        let static_context = context::StaticContext::default();
        let xot = Xot::new();
        let params = SerializationParameters::from_map(map, &static_context, &xot).unwrap();
        assert_eq!(
            params.json_node_output_method,
            QNameOrString::String("xml".to_string())
        );
    }

    #[test]
    fn test_qname_or_string_default_missing() {
        let map = Map::new(vec![]).unwrap();
        let static_context = context::StaticContext::default();
        let xot = Xot::new();
        let params = SerializationParameters::from_map(map, &static_context, &xot).unwrap();
        assert_eq!(
            params.json_node_output_method,
            QNameOrString::String("xml".to_string())
        );
    }
}
