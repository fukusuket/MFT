//! End to end: `mozu collect` where it can run without a live NTFS volume.

use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

type TestResult = Result<(), Box<dyn Error>>;

/// A per-test scratch directory under Cargo's target dir.
fn scratch(test: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(test);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(not(windows))]
#[test]
fn collect_off_windows_fails_with_a_message_and_writes_nothing() -> TestResult {
    let dir = scratch("collect_off_windows")?;
    let zip = dir.join("c.zip");
    let _ = std::fs::remove_file(&zip);
    let out = Command::new(env!("CARGO_BIN_EXE_mozu"))
        .args(["collect", "-o"])
        .arg(&zip)
        .output()?;
    assert!(!out.status.success());
    assert!(String::from_utf8(out.stderr)?.contains("mozu collect runs on Windows only"));
    assert!(std::fs::read_dir(&dir)?.next().is_none());
    Ok(())
}

#[test]
fn collect_refuses_an_existing_output_and_leaves_it_unchanged() -> TestResult {
    let dir = scratch("collect_existing_output")?;
    let zip = dir.join("c.zip");
    std::fs::write(&zip, b"keep")?;
    let out = Command::new(env!("CARGO_BIN_EXE_mozu"))
        .args(["collect", "-o"])
        .arg(&zip)
        .output()?;
    assert!(!out.status.success());
    assert!(String::from_utf8(out.stderr)?.contains("already exists"));
    assert_eq!(std::fs::read(&zip)?, b"keep");
    Ok(())
}

#[test]
fn collect_accepts_only_a_drive_letter_and_a_colon_as_the_volume() -> TestResult {
    let dir = scratch("collect_bad_volume")?;
    let zip = dir.join("c.zip");
    let _ = std::fs::remove_file(&zip);
    for volume in [
        "PhysicalDrive0",
        r"C:\",
        r"GLOBALROOT\Device\X",
        "C:\u{1b}[2J",
        "1:",
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_mozu"))
            .args(["collect", "-v", volume, "-o"])
            .arg(&zip)
            .output()?;
        assert!(!out.status.success());
        let stderr = String::from_utf8(out.stderr)?;
        assert!(stderr.contains("drive letter"), "{volume:?}: {stderr}");
        assert!(!stderr.contains('\u{1b}'));
    }
    assert!(std::fs::read_dir(&dir)?.next().is_none());
    Ok(())
}
