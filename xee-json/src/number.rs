//! The RFC 8259 §6 number grammar, shared by the tokenizer and the writer.
//!
//! ```text
//! number = [ minus ] int [ frac ] [ exp ]
//! int    = zero / ( digit1-9 *DIGIT )
//! frac   = decimal-point 1*DIGIT
//! exp    = e [ minus / plus ] 1*DIGIT
//! ```

/// Scans the number that starts at byte `start` of `bytes`.
///
/// `Ok(end)`: the offset just past the number, which ends at the first byte
/// that cannot continue it (what follows is the caller's concern).
///
/// `Err(offset)`: the offset of the first byte that does not fit the
/// grammar, or `bytes.len()` when the text ends where a digit is required.
/// A leading zero followed by a digit (`01`) is an error at that digit.
pub(crate) fn scan(bytes: &[u8], start: usize) -> Result<usize, usize> {
    let mut pos = start;
    if bytes.get(pos) == Some(&b'-') {
        pos += 1;
    }
    match bytes.get(pos) {
        Some(b'0') => {
            pos += 1;
            if let Some(b'0'..=b'9') = bytes.get(pos) {
                return Err(pos);
            }
        }
        _ => pos = digits(bytes, pos)?,
    }
    if bytes.get(pos) == Some(&b'.') {
        pos = digits(bytes, pos + 1)?;
    }
    if let Some(b'e' | b'E') = bytes.get(pos) {
        pos += 1;
        if let Some(b'+' | b'-') = bytes.get(pos) {
            pos += 1;
        }
        pos = digits(bytes, pos)?;
    }
    Ok(pos)
}

/// One or more digits starting at `pos`: the offset after the last one, or
/// `Err(pos)` if there is none.
fn digits(bytes: &[u8], mut pos: usize) -> Result<usize, usize> {
    if !matches!(bytes.get(pos), Some(b'0'..=b'9')) {
        return Err(pos);
    }
    while let Some(b'0'..=b'9') = bytes.get(pos) {
        pos += 1;
    }
    Ok(pos)
}

/// Whether `text` is exactly one RFC 8259 §6 number, with nothing before or
/// after it (no whitespace either).
pub(crate) fn is_number(text: &str) -> bool {
    scan(text.as_bytes(), 0) == Ok(text.len())
}
