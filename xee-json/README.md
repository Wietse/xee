# xee-json

A strict [RFC 8259](https://www.rfc-editor.org/rfc/rfc8259) JSON pull
tokenizer, as used by the rest of the Xee project.

- Events in document order; duplicate object keys are never merged.
- Strings without escapes are borrowed slices of the input. Strings with
  escapes are validated once and decoded lazily into segments that keep the
  source escape text, so an unpaired surrogate (`\uD800`) is representable.
- Numbers are delivered as their validated lexical text.
- An explicit stack with a mandatory maximum nesting depth (default 512), a
  byte-order-mark policy, and errors with a kind and a byte offset.

It has no dependencies beyond `std`.

This is a low-level crate of the [Xee project](https://github.com/Wietse/xee).
