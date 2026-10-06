//! End to end: the `tool` binary on a synthetic `$MFT`.

#[path = "../../mft-parse/tests/support/mod.rs"]
mod support;

use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

use support::{file_name_with, record, standard_information, u16s};

type TestResult = Result<(), Box<dyn Error>>;

/// A per-test scratch directory under Cargo's target dir.
fn scratch(test: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(test);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Root (5), `\Users`, a file inside it, a file whose parent reference is stale, a BAAD record.
fn small_volume_mft() -> Vec<u8> {
    let root = 5 | (1 << 48);
    let users = 6 | (1 << 48);
    let users_reused = 6 | (2 << 48);
    let named = |entry: u32, parent: u64, name: &str| {
        record(
            1024,
            true,
            entry,
            &[
                standard_information(),
                file_name_with(parent, &u16s(name), 1, 0),
            ],
        )
    };
    let mut mft = named(0, root, "$MFT");
    mft.extend(vec![0u8; 4 * 1024]); // entries 1-4 never used
    mft.extend(named(5, root, "."));
    mft.extend(named(6, root, "Users"));
    mft.extend(named(7, users, "a.txt"));
    mft.extend(named(8, users_reused, "old.txt"));
    let mut bad = record(1024, true, 9, &[standard_information()]);
    bad[0..4].copy_from_slice(b"BAAD");
    mft.extend(bad);
    mft
}

fn tool() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tool"))
}

#[test]
fn analyze_writes_one_csv_row_per_record_with_paths() -> TestResult {
    let dir = scratch("analyze_writes_one_csv_row_per_record_with_paths")?;
    let (input, csv) = (dir.join("MFT"), dir.join("out.csv"));
    std::fs::write(&input, small_volume_mft())?;
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
        "entry,sequence,in_use,name,path,path_state,si_created,fn_created,diagnostics\n\
         0,1,true,$MFT,\\$MFT,resolved,2019-04-17T18:40:00.0000000Z,1601-01-01T00:00:00.0000000Z,\n\
         5,1,true,.,\\,resolved,2019-04-17T18:40:00.0000000Z,1601-01-01T00:00:00.0000000Z,\n\
         6,1,true,Users,\\Users,resolved,2019-04-17T18:40:00.0000000Z,1601-01-01T00:00:00.0000000Z,\n\
         7,1,true,a.txt,\\Users\\a.txt,resolved,2019-04-17T18:40:00.0000000Z,1601-01-01T00:00:00.0000000Z,\n\
         8,1,true,old.txt,,unknown,2019-04-17T18:40:00.0000000Z,1601-01-01T00:00:00.0000000Z,\n\
         9,0,false,,,unknown,,,bad_signature\n"
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
    std::fs::write(&input, small_volume_mft())?;

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
