# Spike S2: `mft` crate (master @ `18b6c05`) on synthetic FILE records

Throwaway. Result recorded in [ADR 0004](../../docs/adr/0004-mft-parser.md).

## Run

```sh
cargo run --release                                        # functional checks, exit 1 on any FAIL
PROPTEST_CASES=100000000 cargo test --release -- --test-threads=3   # no-panic run (~30 min)
```

`src/lib.rs` builds FILE records (header, update sequence array with 512-byte stride, resident `$STANDARD_INFORMATION`, `$FILE_NAME`, `$DATA`) at 1024 and 4096 bytes. No real evidence is used.

## Results (macOS, Apple Silicon, rustc 1.98.1, release)

| Criterion | Result |
|---|---|
| Unpaired surrogates survive | Pass via `Utf16LeStr::as_utf16le_bytes()`: `[0061, D800, 0062]`. **`to_utf8_string()` silently drops the surrogate (`"ab"`)** |
| 4Kn (4096-byte records) | Pass: parses, `valid_fixup=Some(true)`, all attributes read |
| Deleted entries | Pass: `is_allocated=false`, attributes still readable |
| ADS | Pass: named `$DATA` `Zone.Identifier` with content |
| Resident data | Pass: unnamed `$DATA` = `"hello"` |
| Torn write | Pass: corrupted sector → `valid_fixup=Some(false)`, no panic |
| No panic (proptest, option b) | Pass: **3 properties × 100,000,000 cases (300M total) in 1,711 s (28.5 min), 0 panics** |

```
[PASS] 1024-byte record parses: valid_fixup=Some(true)
[PASS] 1024: unpaired surrogate survives (raw units): units=[0061, D800, 0062] to_utf8_string="ab"
[PASS] 1024: resident main $DATA: Some("hello")
[PASS] 1024: ADS name and content: Some("Zone.Identifier")
[PASS] 4096-byte record parses: valid_fixup=Some(true)
[PASS] 4096: unpaired surrogate survives (raw units): units=[0061, D800, 0062] to_utf8_string="ab"
[PASS] 4096: resident main $DATA: Some("hello")
[PASS] 4096: ADS name and content: Some("Zone.Identifier")
[PASS] deleted entry reported: is_allocated=false names=["gone.exe"]
[PASS] torn sector detected: valid_fixup=Some(false)
```

## No-panic properties (`tests/no_panic.rs`)
Each parsed entry is fully walked, including the `utf16-simd` paths (`to_utf8_string`, `Display`, serde) that see attacker-controlled names.
1. `arbitrary_bytes`: any bytes, 0–4200 long (mostly rejected at the signature; shallow).
2. `corrupted_valid_records`: valid 1024/4096 records with 1–15 random byte overwrites (deep).
3. `arbitrary_names`: valid records whose `$FILE_NAME` is any `u16` sequence (drives `utf16-simd`).

## Findings
1. Build `NtfsName` only from `as_utf16le_bytes()`; never from `to_utf8_string`/`Display` (lossy).
2. `utf16-simd` (75 `unsafe`) sits only on those UTF-8 conversion paths; our pipeline doesn't need them, but they are linked in.
3. Fixup handling is bounds-checked on master (issue #129 appears addressed).
4. Not covered: non-resident attributes, `$ATTRIBUTE_LIST`, real 4Kn volumes (no hardware), cargo-fuzz (Phase 1).
