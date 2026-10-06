//! Any bytes as a whole `$MFT`: `records` must never panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(records) = mft_parse::records(data) {
        for item in records {
            let _ = item;
        }
    }
});
