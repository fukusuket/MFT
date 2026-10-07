//! VanillaWindowsReference CSV (`Get-ChildItem` export of a clean install, files only).

use std::collections::BTreeSet;
use std::io::Read;

use crate::Error;
use crate::normalize::key;

/// Root files the VWR author adds while collecting; never part of Windows.
const RESEARCH_ARTIFACTS: [&str; 2] = ["PsExec_IgnoreThisFile_ResearchTool.exe", "test.csv"];

/// Normalized keys of every `C:\` file in the CSV, sorted and without duplicates.
pub(crate) fn keys(csv: impl Read) -> Result<BTreeSet<Vec<u16>>, Error> {
    let mut reader = csv::Reader::from_reader(csv);
    let full_name = reader
        .headers()?
        .iter()
        .position(|h| h.trim_start_matches('\u{feff}') == "FullName")
        .ok_or(Error::NoFullNameColumn)?;
    let mut keys = BTreeSet::new();
    for record in reader.records() {
        let record = record?;
        let Some(path) = record.get(full_name).and_then(|f| f.strip_prefix(r"C:\")) else {
            continue;
        };
        if RESEARCH_ARTIFACTS.contains(&path) {
            continue;
        }
        let segments: Vec<Vec<u16>> = path
            .split('\\')
            .map(|s| s.encode_utf16().collect())
            .collect();
        let refs: Vec<&[u16]> = segments.iter().map(Vec::as_slice).collect();
        keys.extend(key(&refs));
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = r#""DirectoryName","Name","FullName","Length"
"C:\","PsExec_IgnoreThisFile_ResearchTool.exe","C:\PsExec_IgnoreThisFile_ResearchTool.exe","1"
"C:\","test.csv","C:\test.csv","1"
"C:\Windows","notepad.exe","C:\Windows\notepad.exe","1"
"C:\Users\alice","a, b.txt","C:\Users\alice\a, b.txt","1"
"C:\Users\bob","a, b.txt","C:\Users\bob\a, b.txt","1"
"D:\x","y","D:\x\y","1"
"#;

    #[test]
    fn reads_full_names_under_c_and_skips_research_artifacts() -> Result<(), Error> {
        let shown: Vec<String> = keys(CSV.as_bytes())?
            .iter()
            .map(|k| String::from_utf16_lossy(k).replace('\0', "␀"))
            .collect();

        assert_eq!(shown, [r"USERS\␀USER\A, B.TXT", r"WINDOWS\NOTEPAD.EXE"]);
        Ok(())
    }

    #[test]
    fn finds_full_name_by_header_and_rejects_csv_without_it() -> Result<(), Error> {
        let reordered = "\u{feff}\"FullName\",\"Name\"\n\"C:\\Windows\\a.exe\",\"a.exe\"\n";
        let shown: Vec<String> = keys(reordered.as_bytes())?
            .iter()
            .map(|k| String::from_utf16_lossy(k))
            .collect();
        assert_eq!(shown, [r"WINDOWS\A.EXE"]);

        let without = "\"DirectoryName\",\"Name\"\n\"C:\\Windows\",\"a.exe\"\n";
        assert!(matches!(
            keys(without.as_bytes()),
            Err(Error::NoFullNameColumn)
        ));
        Ok(())
    }
}
