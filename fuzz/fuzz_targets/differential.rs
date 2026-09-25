#![no_main]
//! xee-json against serde_json, with the listed divergences; see
//! `xee_json_fuzz::differential`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| xee_json_fuzz::differential(data));
