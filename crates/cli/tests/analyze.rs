//! End to end: the `tool` binary on a synthetic `$MFT`.

#[path = "../../mft-parse/tests/support/mod.rs"]
mod support;

use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

use support::{file_name_with, record, standard_information, standard_information_created, u16s};

type TestResult = Result<(), Box<dyn Error>>;

/// A per-test scratch directory under Cargo's target dir.
fn scratch(test: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(test);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn three_record_mft() -> Vec<u8> {
    let mut mft = record(
        1024,
        true,
        0,
        &[
            standard_information_created(133_444_555_666_777_888),
            file_name_with(5 | (5 << 48), &u16s("a.txt"), 1, 133_536_836_961_234_567),
        ],
    );
    mft.extend(record(1024, false, 1, &[standard_information()]));
    let mut bad = record(1024, true, 2, &[standard_information()]);
    bad[0..4].copy_from_slice(b"BAAD");
    mft.extend(bad);
    mft
}

fn tool() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tool"))
}

#[test]
fn analyze_writes_one_csv_row_per_record() -> TestResult {
    let dir = scratch("analyze_writes_one_csv_row_per_record")?;
    let (input, csv) = (dir.join("MFT"), dir.join("out.csv"));
    std::fs::write(&input, three_record_mft())?;
    let _ = std::fs::remove_file(&csv);

    let status = tool()
        .arg("analyze")
        .arg("-i")
        .arg(&input)
        .arg("--csv")
        .arg(&csv)
        .status()?;

    assert!(status.success());
    assert_eq!(
        std::fs::read_to_string(&csv).unwrap_or_default(),
        "entry,sequence,in_use,name,si_created,fn_created,diagnostics\n\
         0,1,true,a.txt,2023-11-14T17:12:46.6777888Z,2024-02-29T12:34:56.1234567Z,\n\
         1,1,false,,2019-04-17T18:40:00.0000000Z,,\n\
         2,0,false,,,,bad_signature\n"
    );
    Ok(())
}

#[test]
fn missing_input_fails_with_a_message_not_a_panic() -> TestResult {
    let dir = scratch("missing_input_fails_with_a_message_not_a_panic")?;

    let out = tool()
        .arg("analyze")
        .arg("-i")
        .arg(dir.join("no-such-MFT"))
        .arg("--csv")
        .arg(dir.join("out.csv"))
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("opening") && stderr.contains("no-such-MFT"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");
    Ok(())
}

#[test]
fn same_input_gives_byte_identical_csv() -> TestResult {
    let dir = scratch("same_input_gives_byte_identical_csv")?;
    let input = dir.join("MFT");
    std::fs::write(&input, three_record_mft())?;

    let mut outputs = Vec::new();
    for run in ["1.csv", "2.csv"] {
        let csv = dir.join(run);
        let status = tool()
            .arg("analyze")
            .arg("-i")
            .arg(&input)
            .arg("--csv")
            .arg(&csv)
            .status()?;
        assert!(status.success());
        outputs.push(std::fs::read(&csv)?);
    }

    assert_eq!(outputs[0], outputs[1]);
    Ok(())
}
