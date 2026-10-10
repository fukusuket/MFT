//! Any bytes as a whole `$J`: `records` must never panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for item in usn_parse::records(data) {
        let _ = item;
    }
});
