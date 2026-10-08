//! The `xtask` binary end to end.

use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

fn xtask() -> Command {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
}

#[test]
fn synth_mft_writes_the_asked_number_of_records() -> Result<(), Box<dyn Error>> {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("xtask-synth-MFT");
    let _ = std::fs::remove_file(&out);

    let status = xtask().args(["synth-mft", "100"]).arg(&out).status()?;

    assert!(status.success());
    assert_eq!(std::fs::metadata(&out)?.len(), 100 * 1024);
    Ok(())
}

#[test]
fn wrong_arguments_give_a_usage_error() -> Result<(), Box<dyn Error>> {
    for args in [
        &[][..],
        &["synth-mft"],
        &["synth-mft", "many", "x"],
        &["other", "1", "x"],
    ] {
        let out = xtask().args(args).output()?;

        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {stderr}");
        assert!(
            stderr.contains("usage: xtask synth-mft <records> <out>"),
            "{stderr}"
        );
    }
    Ok(())
}
