#![no_main]
//! No panic and no stack overflow compiling any pattern under any flags;
//! see `xee_json_fuzz::regex::regex_parse`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| xee_json_fuzz::regex::regex_parse(data));
