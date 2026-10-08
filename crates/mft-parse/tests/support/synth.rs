//! A synthetic volume's `$MFT`, built with the record builder, for scale and determinism tests
//! (Phase 1 gate). Pure arithmetic, no randomness: the same count gives the same bytes.

use std::io::Write;

use super::{
    extension, file_name_with, record, record_with_flags, standard_information_created, u16s,
};

/// Bytes per record.
pub(crate) const RECORD: usize = 1024;

/// Entries 0-15 except the root (5).
const METAFILES: [&str; 16] = [
    "$MFT", "$MFTMirr", "$LogFile", "$Volume", "$AttrDef", ".", "$Bitmap", "$Boot", "$BadClus",
    "$Secure", "$UpCase", "$Extend", "", "", "", "",
];
const ROOT: u32 = 5;
/// Fixed directories after the metafiles: (entry, parent, name).
const TOP: [(u32, u32, &str); 7] = [
    (16, ROOT, "Windows"),
    (17, ROOT, "Program Files"),
    (18, ROOT, "ProgramData"),
    (19, ROOT, "Users"),
    (20, 19, "Public"),
    (21, 19, "alice"),
    (22, 16, "System32"),
];
const PUBLIC: u32 = 20;
const EXTENSIONS: [&str; 5] = ["txt", "dll", "exe", "log", "dat"];
/// 2023-06-15, and two years of 100 ns ticks.
const EPOCH: u64 = 133_313_184_000_000_000;
const TWO_YEARS: u64 = 630_720_000_000_000;

/// Writes `records` records to `out`.
pub(crate) fn synthetic_mft(records: u32, out: &mut impl Write) -> std::io::Result<()> {
    let mut dirs: Vec<u32> = TOP.iter().map(|&(entry, _, _)| entry).collect();
    for i in 0..records {
        let bytes = if let Some(name) = METAFILES.get(index(i)) {
            if i == ROOT {
                directory(i, ROOT, name)
            } else if name.is_empty() {
                vec![0u8; RECORD] // reserved, never used
            } else {
                file(i, ROOT, name, &[])
            }
        } else if let Some(&(entry, parent, name)) = TOP.iter().find(|t| t.0 == i) {
            directory(entry, parent, name)
        } else {
            let parent = parent_for(&dirs, i);
            let ext = EXTENSIONS[index(i / 3) % EXTENSIONS.len()];
            let dos = u16s(&format!("F{}~1.{}", i % 100_000, ext.to_uppercase()));
            if i % 1000 == 999 {
                let mut bad = file(i, parent, &format!("f{i}.{ext}"), &[]);
                bad[0..4].copy_from_slice(b"BAAD");
                bad
            } else if i % 20 == 0 {
                dirs.push(i);
                directory(i, parent, &format!("d{i}"))
            } else if i % 500 == 3 {
                // Planted for the sample rules: Public executable, system name, double extension.
                file(i, PUBLIC, &format!("p{i}.exe"), &[])
            } else if i % 1000 == 13 {
                file(i, parent, "svchost.exe", &[])
            } else if i % 1000 == 23 {
                file(i, parent, &format!("invoice{i}.pdf.exe"), &[])
            } else if i % 100 == 49 {
                // The long name lives in the next record, an extension of this one.
                dos_only(i, parent, &dos)
            } else if i % 100 == 50 {
                extension(
                    RECORD,
                    true,
                    i,
                    reference(i - 1),
                    &[file_name_with(
                        reference(parent_for(&dirs, i - 1)),
                        &u16s(&format!("long name {}.txt", i - 1)),
                        1,
                        fn_created(i - 1),
                    )],
                )
            } else if i % 20 == 7 {
                file_in_use(false, i, parent, &format!("f{i}.{ext}"), &[])
            } else if i % 10 < 3 {
                file(i, parent, &format!("f{i}.{ext}"), &dos)
            } else {
                file(i, parent, &format!("f{i}.{ext}"), &[])
            }
        };
        out.write_all(&bytes)?;
    }
    Ok(())
}

/// A directory picked from those made so far; at 1M records paths are mostly 8-9 deep, at most ~20.
fn parent_for(dirs: &[u32], entry: u32) -> u32 {
    dirs[index(entry).wrapping_mul(2_654_435_761) % dirs.len()]
}

fn index(i: u32) -> usize {
    usize::try_from(i).unwrap_or_default()
}

fn reference(entry: u32) -> u64 {
    u64::from(entry) | (1 << 48)
}

/// `$SI` created time, spread over two years.
fn si_created(entry: u32) -> u64 {
    EPOCH + u64::from(entry).wrapping_mul(7_919_000_000_017) % TWO_YEARS
}

/// `$FN` created time, spread differently from `$SI`.
fn fn_created(entry: u32) -> u64 {
    EPOCH + u64::from(entry).wrapping_mul(6_007_000_000_011) % TWO_YEARS
}

fn directory(entry: u32, parent: u32, name: &str) -> Vec<u8> {
    record_with_flags(
        RECORD,
        0x03,
        entry,
        &[
            standard_information_created(si_created(entry)),
            file_name_with(reference(parent), &u16s(name), 1, fn_created(entry)),
        ],
    )
}

/// An in-use file with a Win32 name and, if given, a DOS name.
fn file(entry: u32, parent: u32, name: &str, dos: &[u16]) -> Vec<u8> {
    file_in_use(true, entry, parent, name, dos)
}

fn file_in_use(in_use: bool, entry: u32, parent: u32, name: &str, dos: &[u16]) -> Vec<u8> {
    let mut attrs = vec![
        standard_information_created(si_created(entry)),
        file_name_with(reference(parent), &u16s(name), 1, fn_created(entry)),
    ];
    if !dos.is_empty() {
        attrs.push(file_name_with(reference(parent), dos, 2, fn_created(entry)));
    }
    record(RECORD, in_use, entry, &attrs)
}

/// A file whose only name in its base record is the DOS one.
fn dos_only(entry: u32, parent: u32, dos: &[u16]) -> Vec<u8> {
    record(
        RECORD,
        true,
        entry,
        &[
            standard_information_created(si_created(entry)),
            file_name_with(reference(parent), dos, 2, fn_created(entry)),
        ],
    )
}
