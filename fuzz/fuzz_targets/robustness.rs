#![no_main]
//! No panic on any input, a bounded and fused drain, a balanced event
//! stream on success; see `xee_json_fuzz::robustness`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| xee_json_fuzz::robustness(data));
