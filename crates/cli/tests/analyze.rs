//! End to end: the `tool` binary on a synthetic `$MFT`.

#[path = "../../mft-parse/tests/support/mod.rs"]
mod support;

use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

use support::{extension, file_name_with, record, record_with_flags, standard_information, u16s};

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

fn assert_no_temporary_outputs(dir: &std::path::Path) -> TestResult {
    for entry in std::fs::read_dir(dir)? {
        let name = entry?.file_name();
        assert!(!name.to_string_lossy().contains(".tool-tmp-"), "{name:?}");
    }
    Ok(())
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
        "entry,sequence,in_use,name,path,path_state,baseline,findings,si_created,fn_created,diagnostics\n\
         0,1,true,$MFT,\\$MFT,resolved,,,2019-04-17T18:40:00.0000000Z,1601-01-01T00:00:00.0000000Z,\n\
         5,1,true,.,\\,resolved,,,2019-04-17T18:40:00.0000000Z,1601-01-01T00:00:00.0000000Z,\n\
         6,1,true,Users,\\Users,resolved,,,2019-04-17T18:40:00.0000000Z,1601-01-01T00:00:00.0000000Z,\n\
         7,1,true,a.txt,\\Users\\a.txt,resolved,,,2019-04-17T18:40:00.0000000Z,1601-01-01T00:00:00.0000000Z,\n\
         8,1,true,old.txt,,unknown,,,2019-04-17T18:40:00.0000000Z,1601-01-01T00:00:00.0000000Z,\n\
         9,0,false,,,unknown,,,,,bad_signature\n"
    );
    assert_no_temporary_outputs(&dir)?;
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
fn analyze_refuses_to_overwrite_its_mft_with_csv() -> TestResult {
    let dir = scratch("analyze_refuses_to_overwrite_its_mft_with_csv")?;
    let input = dir.join("MFT");
    let original = small_volume_mft();
    std::fs::write(&input, &original)?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(&input)
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("same file"), "{stderr}");
    assert_eq!(std::fs::read(&input)?, original);
    Ok(())
}

#[test]
fn analyze_refuses_to_overwrite_its_mft_with_html() -> TestResult {
    let dir = scratch("analyze_refuses_to_overwrite_its_mft_with_html")?;
    let input = dir.join("MFT");
    let original = small_volume_mft();
    std::fs::write(&input, &original)?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("-o")
        .arg(&input)
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("same file"), "{stderr}");
    assert_eq!(std::fs::read(&input)?, original);
    Ok(())
}

#[test]
fn analyze_refuses_an_equivalent_path_to_its_mft() -> TestResult {
    let dir = scratch("analyze_refuses_an_equivalent_path_to_its_mft")?;
    let input = dir.join("MFT");
    let original = small_volume_mft();
    std::fs::create_dir_all(dir.join("subdir"))?;
    std::fs::write(&input, &original)?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(dir.join("subdir").join("..").join("MFT"))
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("same file"), "{stderr}");
    assert_eq!(std::fs::read(&input)?, original);
    Ok(())
}

#[test]
fn analyze_refuses_the_same_file_for_csv_and_html() -> TestResult {
    let dir = scratch("analyze_refuses_the_same_file_for_csv_and_html")?;
    let (input, output) = (dir.join("MFT"), dir.join("report"));
    std::fs::write(&input, small_volume_mft())?;
    std::fs::write(&output, b"keep this report")?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(&output)
        .arg("-o")
        .arg(&output)
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("same file"), "{stderr}");
    assert_eq!(std::fs::read(&output)?, b"keep this report");
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

#[test]
fn baseline_build_then_analyze_labels_files() -> TestResult {
    let dir = scratch("baseline_build_then_analyze_labels_files")?;
    let (vwr, index, input, csv) = (
        dir.join("vwr.csv"),
        dir.join("b.fst"),
        dir.join("MFT"),
        dir.join("out.csv"),
    );
    std::fs::write(
        &vwr,
        "\"DirectoryName\",\"Name\",\"FullName\"\n\"C:\\\",\"$MFT\",\"C:\\$MFT\"\n\"C:\\Users\",\"Bob\",\"C:\\Users\\Bob\"\n",
    )?;
    std::fs::write(&input, small_volume_mft())?;

    let built = tool()
        .args(["baseline", "build", "--vwr"])
        .arg(&vwr)
        .arg("-o")
        .arg(&index)
        .status()?;
    let analyzed = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(&csv)
        .arg("--baseline")
        .arg(&index)
        .status()?;

    assert!(built.success() && analyzed.success());
    let text = std::fs::read_to_string(&csv).unwrap_or_default();
    let column: Vec<&str> = text
        .lines()
        .skip(1)
        .map(|r| r.split(',').nth(6).unwrap_or_default())
        .collect();
    // $MFT; root; \Users (a file in this fixture); \Users\a.txt (directly under \Users, so literal
    // and not the VWR's \Users\Bob, ADR 0015); stale; BAAD
    assert_eq!(column, ["standard", "", "outside", "outside", "", ""]);
    Ok(())
}

#[test]
fn baseline_build_refuses_to_overwrite_its_vwr_csv() -> TestResult {
    let dir = scratch("baseline_build_refuses_to_overwrite_its_vwr_csv")?;
    let vwr = dir.join("vwr.csv");
    let original = b"\"FullName\"\n\"C:\\Windows\\notepad.exe\"\n";
    std::fs::write(&vwr, original)?;

    let out = tool()
        .args(["baseline", "build", "--vwr"])
        .arg(&vwr)
        .arg("-o")
        .arg(&vwr)
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("same file"), "{stderr}");
    assert_eq!(std::fs::read(&vwr)?, original);
    Ok(())
}

#[test]
fn baseline_build_failure_preserves_an_existing_output() -> TestResult {
    let dir = scratch("baseline_build_failure_preserves_an_existing_output")?;
    let (vwr, output) = (dir.join("vwr.csv"), dir.join("baseline.fst"));
    std::fs::write(&vwr, b"\"FullName\"\n\xff\n")?;
    std::fs::write(&output, b"previous baseline")?;

    let out = tool()
        .args(["baseline", "build", "--vwr"])
        .arg(&vwr)
        .arg("-o")
        .arg(&output)
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("VanillaWindowsReference CSV"), "{stderr}");
    assert_eq!(std::fs::read(&output)?, b"previous baseline");
    assert_no_temporary_outputs(&dir)?;
    Ok(())
}

#[test]
fn bad_baseline_file_fails_with_a_message_not_a_panic() -> TestResult {
    let dir = scratch("bad_baseline_file_fails_with_a_message_not_a_panic")?;
    let (index, input) = (dir.join("not-a-baseline.fst"), dir.join("MFT"));
    std::fs::write(&index, b"definitely not a baseline")?;
    std::fs::write(&input, small_volume_mft())?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(dir.join("out.csv"))
        .arg("--baseline")
        .arg(&index)
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("not-a-baseline.fst") && stderr.contains("not a baseline file"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");
    Ok(())
}

#[test]
fn analyze_failure_preserves_an_existing_csv() -> TestResult {
    let dir = scratch("analyze_failure_preserves_an_existing_csv")?;
    let (input, csv) = (dir.join("MFT"), dir.join("out.csv"));
    let mut unsupported = vec![0u8; 1024];
    unsupported[0x1c..0x20].copy_from_slice(&2048u32.to_le_bytes());
    std::fs::write(&input, unsupported)?;
    std::fs::write(&csv, b"previous report")?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(&csv)
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("unsupported $MFT record size"), "{stderr}");
    assert_eq!(std::fs::read(&csv)?, b"previous report");
    assert_no_temporary_outputs(&dir)?;
    Ok(())
}

#[test]
fn analyze_failure_preserves_an_existing_html_report() -> TestResult {
    let dir = scratch("analyze_failure_preserves_an_existing_html_report")?;
    let (input, html) = (dir.join("MFT"), dir.join("report.html"));
    let mut unsupported = vec![0u8; 1024];
    unsupported[0x1c..0x20].copy_from_slice(&2048u32.to_le_bytes());
    std::fs::write(&input, unsupported)?;
    std::fs::write(&html, b"previous report")?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("-o")
        .arg(&html)
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("unsupported $MFT record size"), "{stderr}");
    assert_eq!(std::fs::read(&html)?, b"previous report");
    assert_no_temporary_outputs(&dir)?;
    Ok(())
}

#[test]
fn extension_records_merge_into_one_row() -> TestResult {
    let dir = scratch("extension_records_merge_into_one_row")?;
    let (input, csv) = (dir.join("MFT"), dir.join("out.csv"));
    let root = 5 | (1 << 48);
    let file = 6 | (1 << 48);
    let mut mft = record(
        1024,
        true,
        0,
        &[
            standard_information(),
            file_name_with(root, &u16s("$MFT"), 1, 0),
        ],
    );
    mft.extend(vec![0u8; 4 * 1024]);
    mft.extend(record(
        1024,
        true,
        5,
        &[
            standard_information(),
            file_name_with(root, &u16s("."), 1, 0),
        ],
    ));
    mft.extend(record(
        1024,
        true,
        6,
        &[
            standard_information(),
            file_name_with(root, &u16s("LONGNA~1.TXT"), 2, 0),
        ],
    ));
    mft.extend(extension(
        1024,
        true,
        7,
        file,
        &[file_name_with(root, &u16s("Long name.txt"), 1, 0)],
    ));
    mft.extend(extension(
        1024,
        false,
        8,
        file,
        &[file_name_with(root, &u16s("stale.txt"), 1, 0)],
    ));
    std::fs::write(&input, mft)?;

    let status = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(&csv)
        .status()?;

    assert!(status.success());
    let text = std::fs::read_to_string(&csv).unwrap_or_default();
    let rows: Vec<(String, String, String)> = text
        .lines()
        .skip(1)
        .map(|r| {
            let f: Vec<&str> = r.split(',').collect();
            (
                f[0].to_string(),
                f[4].to_string(),
                f[f.len() - 1].to_string(),
            )
        })
        .collect();
    let row = |e: &str, p: &str, d: &str| (e.to_string(), p.to_string(), d.to_string());
    assert_eq!(
        rows,
        [
            row("0", r"\$MFT", ""),
            row("5", r"\", ""),
            row("6", r"\Long name.txt", ""),
            row("8", r"\stale.txt", "orphan_extension"),
        ]
    );
    Ok(())
}

const SAMPLE_RULES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/rules");

/// A `$MFT` with one file per sample rule under `\Windows` and `\Users\Public`, and a baseline
/// file that knows only `C:\Windows\notepad.exe`. Returns the `$MFT` and baseline paths.
fn sample_volume(dir: &std::path::Path) -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    let input = dir.join("MFT");
    let at = |parent: u64| parent | (1 << 48);
    let named = |entry: u32, parent: u64, name: &str| {
        record(
            1024,
            true,
            entry,
            &[
                standard_information(),
                file_name_with(at(parent), &u16s(name), 1, 0),
            ],
        )
    };
    let dir_record = |entry: u32, parent: u64, name: &str| {
        record_with_flags(
            1024,
            0x03,
            entry,
            &[
                standard_information(),
                file_name_with(at(parent), &u16s(name), 1, 0),
            ],
        )
    };
    let mut mft = named(0, 5, "$MFT");
    mft.extend(vec![0u8; 4 * 1024]);
    mft.extend(dir_record(5, 5, "."));
    mft.extend(dir_record(6, 5, "Windows"));
    mft.extend(dir_record(7, 5, "Users"));
    mft.extend(dir_record(8, 7, "Public"));
    mft.extend(named(9, 8, "a.exe"));
    mft.extend(named(10, 6, "svchost.exe"));
    mft.extend(named(11, 8, "invoice.pdf.exe"));
    mft.extend(named(12, 6, "notepad.exe"));
    std::fs::write(&input, mft)?;
    Ok((input, notepad_baseline(dir)?))
}

/// A baseline file that knows only `C:\Windows\notepad.exe`.
fn notepad_baseline(dir: &std::path::Path) -> Result<PathBuf, Box<dyn Error>> {
    let (vwr, index) = (dir.join("vwr.csv"), dir.join("b.fst"));
    std::fs::write(&vwr, "\"FullName\"\n\"C:\\Windows\\notepad.exe\"\n")?;
    let built = tool()
        .args(["baseline", "build", "--vwr"])
        .arg(&vwr)
        .arg("-o")
        .arg(&index)
        .status()?;
    if !built.success() {
        return Err("baseline build failed".into());
    }
    Ok(index)
}

#[test]
fn sample_rules_find_what_they_describe() -> TestResult {
    let dir = scratch("sample_rules_find_what_they_describe")?;
    let (input, index) = sample_volume(&dir)?;
    let csv = dir.join("out.csv");

    let analyzed = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(&csv)
        .arg("--baseline")
        .arg(&index)
        .arg("--rules")
        .arg(SAMPLE_RULES)
        .status()?;

    assert!(analyzed.success());
    let text = std::fs::read_to_string(&csv).unwrap_or_default();
    let findings: Vec<(&str, &str)> = text
        .lines()
        .skip(1)
        .map(|r| {
            let f: Vec<&str> = r.split(',').collect();
            (f[4], f[7])
        })
        .filter(|(_, findings)| !findings.is_empty())
        .collect();
    const PUBLIC: &str = "92986d59-a4d2-45b3-a224-c8a9809b78aa";
    const SYSTEM: &str = "46af8dfb-f067-417d-8837-9e5a1785fee0";
    const DOUBLE: &str = "8839b831-0916-4876-82aa-82e2a9b2f138";
    const OUTSIDE: &str = "290fe785-b499-4e25-b00f-70a4ae863c2f";
    assert_eq!(
        findings,
        [
            (r"\Users\Public\a.exe", format!("medium:{PUBLIC}").as_str()),
            (
                r"\Windows\svchost.exe",
                format!("high:{OUTSIDE};high:{SYSTEM}").as_str()
            ),
            (
                r"\Users\Public\invoice.pdf.exe",
                format!("medium:{DOUBLE};medium:{PUBLIC}").as_str()
            ),
        ]
    );
    Ok(())
}

#[test]
fn a_rule_without_id_fails_with_a_message_not_a_panic() -> TestResult {
    let dir = scratch("a_rule_without_id_fails_with_a_message_not_a_panic")?;
    let rules = dir.join("rules");
    std::fs::create_dir_all(&rules)?;
    std::fs::write(
        rules.join("no_id.yml"),
        "title: No id\nlogsource:\n  category: file_event\ndetection:\n  sel:\n    TargetFilename: x\n  condition: sel\n",
    )?;
    std::fs::write(dir.join("MFT"), small_volume_mft())?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(dir.join("MFT"))
        .arg("--csv")
        .arg(dir.join("out.csv"))
        .arg("--rules")
        .arg(&rules)
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("no_id.yml") && stderr.contains("no id"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");
    Ok(())
}

/// The JSON embedded in a report written by `-o`.
fn report_data(html: &str) -> Result<serde_json::Value, Box<dyn Error>> {
    const OPEN: &str = r#"<script type="application/octet-stream" id="data">"#;
    let start = html.find(OPEN).ok_or("no data element")? + OPEN.len();
    let payload = html[start..].split('<').next().unwrap_or_default();
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (mut bits, mut count, mut bytes) = (0u32, 0, Vec::new());
    for c in payload.bytes().filter(|&c| c != b'=') {
        let value = ALPHABET.iter().position(|&a| a == c).ok_or("not Base64")?;
        bits = bits << 6 | u32::try_from(value)?;
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push(u8::try_from(bits >> count & 0xff)?);
        }
    }
    Ok(serde_json::from_slice(&bytes)?)
}

#[test]
fn html_report_lists_findings_and_the_outside_baseline_files() -> TestResult {
    let dir = scratch("html_report_lists_findings_and_the_outside_baseline_files")?;
    let (input, index) = sample_volume(&dir)?;
    let (csv, html) = (dir.join("out.csv"), dir.join("report.html"));
    let _ = std::fs::remove_file(&html);

    let analyzed = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(&csv)
        .arg("-o")
        .arg(&html)
        .arg("--baseline")
        .arg(&index)
        .arg("--rules")
        .arg(SAMPLE_RULES)
        .status()?;

    assert!(analyzed.success());
    let data = report_data(&std::fs::read_to_string(&html)?)?;
    let mut rule_ids: Vec<&str> = data["findings"]
        .as_array()
        .ok_or("no findings")?
        .iter()
        .filter_map(|f| f["id"].as_str())
        .collect();
    rule_ids.sort_unstable();
    rule_ids.dedup();
    assert_eq!(
        rule_ids,
        [
            "290fe785-b499-4e25-b00f-70a4ae863c2f",
            "46af8dfb-f067-417d-8837-9e5a1785fee0",
            "8839b831-0916-4876-82aa-82e2a9b2f138",
            "92986d59-a4d2-45b3-a224-c8a9809b78aa",
        ]
    );
    let mut outside: Vec<&str> = data["outside"]
        .as_array()
        .ok_or("no outside list")?
        .iter()
        .filter_map(|o| o["path"].as_str())
        .collect();
    outside.sort_unstable();
    let text = std::fs::read_to_string(&csv)?;
    let mut csv_outside: Vec<&str> = text
        .lines()
        .filter_map(|r| {
            let f: Vec<&str> = r.split(',').collect();
            (f.get(6) == Some(&"outside")).then(|| f[4])
        })
        .collect();
    csv_outside.sort_unstable();
    assert_eq!(outside, csv_outside);
    assert_eq!(data["summary"]["input"], "MFT");
    assert_eq!(data["summary"]["baseline"], "b.fst");
    assert_eq!(data["summary"]["rules"], 4);
    let footer = &data["footer"];
    assert_eq!(footer["license"], "AGPL-3.0-only");
    assert_eq!(footer["source_url"], "https://github.com/fukusuket/MFT");
    let commit = footer["commit"].as_str().unwrap_or_default();
    assert!(
        commit == "unknown"
            || (commit.len() == 40 && commit.bytes().all(|c| c.is_ascii_hexdigit())),
        "{commit}"
    );
    Ok(())
}

#[test]
fn analyze_needs_an_output() -> TestResult {
    let dir = scratch("analyze_needs_an_output")?;
    std::fs::write(dir.join("MFT"), small_volume_mft())?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(dir.join("MFT"))
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("--csv") && stderr.contains("--output"),
        "{stderr}"
    );
    Ok(())
}

#[test]
fn synthetic_mft_has_the_asked_size_and_is_reproducible() -> TestResult {
    let (mut first, mut second) = (Vec::new(), Vec::new());

    support::synth::synthetic_mft(3000, &mut first)?;
    support::synth::synthetic_mft(3000, &mut second)?;

    assert_eq!(first.len(), 3000 * support::synth::RECORD);
    assert!(first == second, "two runs differ");
    Ok(())
}

/// CSV rows split into columns (no quoted commas in synthetic names).
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .skip(1)
        .map(|r| r.split(',').map(str::to_string).collect())
        .collect()
}

#[test]
fn synthetic_mft_parses_into_resolved_paths_with_planted_oddities() -> TestResult {
    let dir = scratch("synthetic_mft_parses_into_resolved_paths_with_planted_oddities")?;
    let (input, csv) = (dir.join("MFT"), dir.join("out.csv"));
    let mut mft = Vec::new();
    support::synth::synthetic_mft(3000, &mut mft)?;
    std::fs::write(&input, mft)?;

    let status = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(&csv)
        .status()?;

    assert!(status.success());
    let rows = csv_rows(&std::fs::read_to_string(&csv)?);
    let bad: Vec<&str> = rows
        .iter()
        .filter(|r| !r[10].is_empty())
        .map(|r| r[10].as_str())
        .collect();
    assert_eq!(bad, ["bad_signature"; 3], "diagnostics");
    let unresolved = rows
        .iter()
        .filter(|r| r[5] != "resolved" && r[10].is_empty())
        .count();
    assert_eq!(unresolved, 0);
    let deleted = rows
        .iter()
        .filter(|r| r[2] == "false" && r[10].is_empty())
        .count();
    assert!(deleted > 100, "{deleted} deleted rows");
    let long_names = rows
        .iter()
        .filter(|r| r[3].starts_with("long name "))
        .count();
    assert_eq!(long_names, 30, "names merged from extension records");
    Ok(())
}

#[test]
fn sample_rules_fire_on_the_synthetic_volume() -> TestResult {
    let dir = scratch("sample_rules_fire_on_the_synthetic_volume")?;
    let (input, csv) = (dir.join("MFT"), dir.join("out.csv"));
    let mut mft = Vec::new();
    support::synth::synthetic_mft(3000, &mut mft)?;
    std::fs::write(&input, mft)?;

    let status = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(&csv)
        .arg("--baseline")
        .arg(notepad_baseline(&dir)?)
        .arg("--rules")
        .arg(SAMPLE_RULES)
        .status()?;

    assert!(status.success());
    let text = std::fs::read_to_string(&csv)?;
    for id in [
        "92986d59-a4d2-45b3-a224-c8a9809b78aa",
        "46af8dfb-f067-417d-8837-9e5a1785fee0",
        "8839b831-0916-4876-82aa-82e2a9b2f138",
        "290fe785-b499-4e25-b00f-70a4ae863c2f",
    ] {
        assert!(text.contains(id), "no finding for {id}");
    }
    Ok(())
}

#[test]
fn synthetic_volume_gives_byte_identical_reports() -> TestResult {
    let dir = scratch("synthetic_volume_gives_byte_identical_reports")?;
    let input = dir.join("MFT");
    let mut mft = Vec::new();
    support::synth::synthetic_mft(20_000, &mut mft)?;
    std::fs::write(&input, mft)?;
    let baseline = notepad_baseline(&dir)?;

    let mut outputs = Vec::new();
    for run in ["1", "2"] {
        let (csv, html) = (
            dir.join(format!("{run}.csv")),
            dir.join(format!("{run}.html")),
        );
        let status = tool()
            .args(["analyze", "-i"])
            .arg(&input)
            .arg("--csv")
            .arg(&csv)
            .arg("-o")
            .arg(&html)
            .arg("--baseline")
            .arg(&baseline)
            .arg("--rules")
            .arg(SAMPLE_RULES)
            .status()?;
        assert!(status.success());
        outputs.push((std::fs::read(&csv)?, std::fs::read(&html)?));
    }

    assert!(outputs[0].0 == outputs[1].0, "CSV differs between runs");
    assert!(outputs[0].1 == outputs[1].1, "HTML differs between runs");
    Ok(())
}

fn jsonl_field(text: &str, name: &str) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    text.lines()
        .map(|line| Ok(serde_json::from_str::<serde_json::Value>(line)?[name].take()))
        .collect()
}

#[test]
fn analyze_writes_one_jsonl_line_per_record_with_paths() -> TestResult {
    let dir = scratch("analyze_writes_one_jsonl_line_per_record_with_paths")?;
    let (input, jsonl) = (dir.join("MFT"), dir.join("out.jsonl"));
    std::fs::write(&input, small_volume_mft())?;
    let _ = std::fs::remove_file(&jsonl);

    let out = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--jsonl")
        .arg(&jsonl)
        .output()?;

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&jsonl)?;
    assert_eq!(
        jsonl_field(&text, "path")?,
        [
            serde_json::json!(r"\$MFT"),
            serde_json::json!(r"\"),
            serde_json::json!(r"\Users"),
            serde_json::json!(r"\Users\a.txt"),
            serde_json::Value::Null,
            serde_json::Value::Null,
        ]
    );
    assert_eq!(
        jsonl_field(&text, "diagnostics")?[5],
        serde_json::json!(["bad_signature"])
    );
    assert_no_temporary_outputs(&dir)?;
    Ok(())
}

#[test]
fn analyze_writes_csv_html_and_jsonl_in_one_run() -> TestResult {
    let dir = scratch("analyze_writes_csv_html_and_jsonl_in_one_run")?;
    let input = dir.join("MFT");
    let (csv, html, jsonl) = (dir.join("t.csv"), dir.join("r.html"), dir.join("t.jsonl"));
    std::fs::write(&input, small_volume_mft())?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(&csv)
        .arg("-o")
        .arg(&html)
        .arg("--jsonl")
        .arg(&jsonl)
        .output()?;

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let csv_paths: Vec<String> = csv_rows(&std::fs::read_to_string(&csv)?)
        .into_iter()
        .map(|row| row[4].clone())
        .collect();
    let jsonl_paths: Vec<String> = jsonl_field(&std::fs::read_to_string(&jsonl)?, "path")?
        .iter()
        .map(|p| p.as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(jsonl_paths, csv_paths);
    assert!(report_data(&std::fs::read_to_string(&html)?).is_ok());
    Ok(())
}

#[test]
fn analyze_refuses_to_overwrite_its_mft_with_jsonl() -> TestResult {
    let dir = scratch("analyze_refuses_to_overwrite_its_mft_with_jsonl")?;
    let input = dir.join("MFT");
    let original = small_volume_mft();
    std::fs::write(&input, &original)?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--jsonl")
        .arg(&input)
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("same file"), "{stderr}");
    assert_eq!(std::fs::read(&input)?, original);
    Ok(())
}

#[test]
fn analyze_refuses_the_same_file_for_jsonl_and_another_output() -> TestResult {
    let dir = scratch("analyze_refuses_the_same_file_for_jsonl_and_another_output")?;
    let (input, output) = (dir.join("MFT"), dir.join("report"));
    std::fs::write(&input, small_volume_mft())?;
    std::fs::write(&output, b"keep this report")?;

    for other in ["--csv", "-o"] {
        let out = tool()
            .args(["analyze", "-i"])
            .arg(&input)
            .arg("--jsonl")
            .arg(&output)
            .arg(other)
            .arg(&output)
            .output()?;

        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{other}: {stderr}");
        assert!(stderr.contains("same file"), "{other}: {stderr}");
        assert_eq!(std::fs::read(&output)?, b"keep this report", "{other}");
    }
    Ok(())
}

#[test]
fn analyze_failure_preserves_an_existing_jsonl() -> TestResult {
    let dir = scratch("analyze_failure_preserves_an_existing_jsonl")?;
    let (input, jsonl) = (dir.join("MFT"), dir.join("out.jsonl"));
    let mut unsupported = vec![0u8; 1024];
    unsupported[0x1c..0x20].copy_from_slice(&2048u32.to_le_bytes());
    std::fs::write(&input, unsupported)?;
    std::fs::write(&jsonl, b"previous report")?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--jsonl")
        .arg(&jsonl)
        .output()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("unsupported $MFT record size"), "{stderr}");
    assert_eq!(std::fs::read(&jsonl)?, b"previous report");
    assert_no_temporary_outputs(&dir)?;
    Ok(())
}

#[test]
fn same_input_gives_byte_identical_jsonl() -> TestResult {
    let dir = scratch("same_input_gives_byte_identical_jsonl")?;
    let input = dir.join("MFT");
    let mut mft = Vec::new();
    support::synth::synthetic_mft(20_000, &mut mft)?;
    std::fs::write(&input, mft)?;
    let baseline = notepad_baseline(&dir)?;

    let mut outputs = Vec::new();
    for run in ["1", "2"] {
        let jsonl = dir.join(format!("{run}.jsonl"));
        let status = tool()
            .args(["analyze", "-i"])
            .arg(&input)
            .arg("--jsonl")
            .arg(&jsonl)
            .arg("--baseline")
            .arg(&baseline)
            .arg("--rules")
            .arg(SAMPLE_RULES)
            .status()?;
        assert!(status.success());
        outputs.push(std::fs::read(&jsonl)?);
    }

    assert!(!outputs[0].is_empty());
    assert!(outputs[0] == outputs[1], "JSONL differs between runs");
    Ok(())
}

#[test]
fn sample_rules_skip_system_binaries_in_winsxs() -> TestResult {
    let dir = scratch("sample_rules_skip_system_binaries_in_winsxs")?;
    let (input, csv) = (dir.join("MFT"), dir.join("out.csv"));
    let at = |parent: u64| parent | (1 << 48);
    let entry = |flags: u16, entry: u32, parent: u64, name: &str| {
        record_with_flags(
            1024,
            flags,
            entry,
            &[
                standard_information(),
                file_name_with(at(parent), &u16s(name), 1, 0),
            ],
        )
    };
    let mut mft = entry(0x01, 0, 5, "$MFT");
    mft.extend(vec![0u8; 4 * 1024]);
    mft.extend(entry(0x03, 5, 5, "."));
    mft.extend(entry(0x03, 6, 5, "Windows"));
    mft.extend(entry(0x03, 7, 6, "winsxs"));
    mft.extend(entry(
        0x03,
        8,
        7,
        "amd64_microsoft-windows-lsa_31bf3856ad364e35_6.1.7601.17514_none_04709031736ac277",
    ));
    mft.extend(entry(0x01, 9, 8, "lsass.exe"));
    mft.extend(entry(0x01, 10, 6, "svchost.exe"));
    std::fs::write(&input, mft)?;

    let out = tool()
        .args(["analyze", "-i"])
        .arg(&input)
        .arg("--csv")
        .arg(&csv)
        .arg("--rules")
        .arg(SAMPLE_RULES)
        .output()?;

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let findings: Vec<(String, String)> = csv_rows(&std::fs::read_to_string(&csv)?)
        .into_iter()
        .filter(|row| !row[7].is_empty())
        .map(|row| (row[4].clone(), row[7].clone()))
        .collect();
    assert_eq!(
        findings,
        [(
            r"\Windows\svchost.exe".to_string(),
            "high:46af8dfb-f067-417d-8837-9e5a1785fee0".to_string()
        )]
    );
    Ok(())
}
