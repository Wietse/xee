# xee-json

A strict [RFC 8259](https://www.rfc-editor.org/rfc/rfc8259) JSON pull
tokenizer and a JSON writer, as used by the rest of the Xee project.

The tokenizer:

- Events in document order; duplicate object keys are never merged.
- Strings without escapes are borrowed slices of the input. Strings with
  escapes are validated once and decoded lazily into segments that keep the
  source escape text, so an unpaired surrogate (`\uD800`) is representable.
- Numbers are delivered as their validated lexical text.
- An explicit stack with a mandatory maximum nesting depth (default 512), a
  byte-order-mark policy, and errors with a kind and a byte offset.

The writer:

- One call per token in document order; object members are written in the
  order given, and repeated keys are not merged.
- Number text is supplied by the caller and checked against the RFC 8259
  grammar.
- Strings are escaped by the JSON output method of
  [Serialization 3.1 §9](https://www.w3.org/TR/xslt-xquery-serialization-31/#json-output):
  the solidus and U+007F to U+009F are escaped too, with upper-case
  hexadecimal digits. An "is encodable" predicate escapes the characters an
  output encoding cannot represent (as a surrogate pair outside the Basic
  Multilingual Plane), and verbatim text can be mixed into a string for
  character-map output.
- Compact output (no whitespace outside strings) or indented output.
- Misuse is refused with an error; the tokenizer reads the output back as
  the same events.

It has no dependencies beyond `std`.

This is a low-level crate of the [Xee project](https://github.com/Wietse/xee).
