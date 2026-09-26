//! The tokenizer against serde_json on synthetic RFC 8259 texts.
//!
//! Run with `cargo bench -p xee-json` (all benches) or
//! `cargo bench -p xee-json -- <filter>`; `cargo test -p xee-json --benches`
//! runs each bench once as a smoke test.
//!
//! The inputs, about 1 MiB each, and the two sides live in the [`inputs`]
//! module; the exact size of an input is the throughput counter. Each
//! module below times one input on both sides:
//!
//! - `xee_json` drains a [`Tokenizer`](xee_json::Tokenizer) to its end,
//!   checking every item.
//! - `serde_json` runs `serde_json::from_str::<IgnoredAny>`, which validates
//!   the text (escapes included) without decoding or building anything: the
//!   like-for-like comparison with a drain.
//!
//! Two modules compare more than validation, on inputs chosen so both sides
//! do the same work:
//!
//! - `decode_strings`: every string of an array decoded into a `Vec<String>`
//!   (serde_json's `Vec<String>`; xee-json from the
//!   [`Str`](xee_json::Str) and its segments). The input has no lone
//!   surrogate; the xee-json side would replace one with U+FFFD, the policy
//!   this bench states because the tokenizer leaves it to the consumer.
//! - `borrow_strings`: every string of an escape-free array borrowed into a
//!   `Vec<&str>` (serde_json's `Vec<&str>`; xee-json's
//!   [`Str::Plain`](xee_json::Str::Plain)).
//!
//! Each input passes its guards before it is timed: the guards are defined
//! once in [`inputs`], run when an input is first used, and also run by
//! `tests/bench_inputs.rs` under `cargo test`. A broken generator therefore
//! fails the test suite and the run instead of being timed as a fast
//! rejection.

mod inputs;

use divan::counter::BytesCount;
use divan::{black_box, Bencher};
use inputs::*;

fn main() {
    divan::main();
}

// ============================================================================
// Validation: a drain against IgnoredAny, on each input.
// ============================================================================

/// Registers the two validation benches for one input.
macro_rules! validate {
    ($module:ident, $input:ident) => {
        mod $module {
            use super::*;

            #[divan::bench]
            fn xee_json(bencher: Bencher) {
                let input = $input.as_str();
                bencher
                    .counter(BytesCount::of_str(input))
                    .bench_local(|| xee_drain(black_box(input)).unwrap());
            }

            #[divan::bench]
            fn serde_json(bencher: Bencher) {
                let input = $input.as_str();
                bencher
                    .counter(BytesCount::of_str(input))
                    .bench_local(|| serde_validate(black_box(input)).unwrap());
            }
        }
    };
}

validate!(numbers, NUMBERS);
validate!(object, OBJECT);
validate!(plain_strings, PLAIN_STRINGS);
validate!(one_plain_string, ONE_PLAIN_STRING);
validate!(escaped_strings, ESCAPED_STRINGS);
validate!(deep, DEEP);

// ============================================================================
// Decoding and borrowing strings.
// ============================================================================

/// Escaped strings decoded into a `Vec<String>` on both sides.
mod decode_strings {
    use super::*;

    #[divan::bench]
    fn xee_json(bencher: Bencher) {
        let input = *DECODE_STRINGS;
        bencher
            .counter(BytesCount::of_str(input))
            .bench_local(|| xee_decode_strings(black_box(input)).unwrap());
    }

    #[divan::bench]
    fn serde_json(bencher: Bencher) {
        let input = *DECODE_STRINGS;
        bencher
            .counter(BytesCount::of_str(input))
            .bench_local(|| serde_json::from_str::<Vec<String>>(black_box(input)).unwrap());
    }
}

/// Escape-free strings borrowed into a `Vec<&str>` on both sides.
mod borrow_strings {
    use super::*;

    #[divan::bench]
    fn xee_json(bencher: Bencher) {
        let input = *BORROW_STRINGS;
        bencher
            .counter(BytesCount::of_str(input))
            .bench_local(|| xee_borrow_strings(black_box(input)).unwrap());
    }

    #[divan::bench]
    fn serde_json(bencher: Bencher) {
        let input = *BORROW_STRINGS;
        bencher
            .counter(BytesCount::of_str(input))
            .bench_local(|| serde_json::from_str::<Vec<&str>>(black_box(input)).unwrap());
    }
}
