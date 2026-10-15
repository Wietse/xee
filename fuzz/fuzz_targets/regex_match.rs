#![no_main]
//! The PikeVM and the backtracker agree on every match of any pattern on
//! any input; see `xee_json_fuzz::regex::regex_match`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| xee_json_fuzz::regex::regex_match(data));
