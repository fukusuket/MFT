//! Phase 0 spike S4: how many files on a real volume are outside the Windows baseline?
//! Usage: `cargo run --release -- <$MFT> <VanillaWindowsReference.csv>` (see README.md).
//! Prints aggregates only (no user names or file names).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;

use mft::MftEntry;
use mft::attribute::MftAttributeContent;
use mft::attribute::x30::FileNamespace;

const ROOT: u64 = 5;
const FIRST_USER_RECORD: u64 = 24;

/// One in-use FILE record: its sequence, parent reference and Win32 name.
struct Node {
    seq: u16,
    parent: (u64, u16),
    name: String,
    dir: bool,
    /// `$STANDARD_INFORMATION` created time, Unix seconds.
    created: i64,
}

fn load_mft(path: &str) -> Result<(HashMap<u64, Node>, usize), Box<dyn Error>> {
    let data = std::fs::read(path)?;
    let mut nodes = HashMap::new();
    let mut corrupt = 0usize;
    for (i, rec) in data.as_chunks::<1024>().0.iter().enumerate() {
        let n = u64::try_from(i)?;
        let Ok(entry) = MftEntry::from_buffer(rec.to_vec(), n) else {
            corrupt += 1;
            continue;
        };
        if !entry.is_allocated() || entry.header.base_reference.entry != 0 {
            continue; // deleted, or an extension record of another file
        }
        let mut best: Option<(u8, (u64, u16), String)> = None;
        let mut created = 0i64;
        for attr in entry.iter_attributes().flatten() {
            if let MftAttributeContent::AttrX10(si) = &attr.data {
                created = si.created.as_second();
            }
            if let MftAttributeContent::AttrX30(f) = &attr.data {
                let rank = match f.namespace {
                    FileNamespace::Win32 | FileNamespace::Win32AndDos => 2,
                    FileNamespace::POSIX => 1,
                    FileNamespace::DOS => 0,
                };
                let units: Vec<u16> = f
                    .name
                    .as_utf16le_bytes()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&c| u16::from_le_bytes(c))
                    .collect();
                if best.as_ref().is_none_or(|b| rank > b.0) {
                    best = Some((
                        rank,
                        (f.parent.entry, f.parent.sequence),
                        String::from_utf16_lossy(&units),
                    ));
                }
            }
        }
        if let Some((_, parent, name)) = best {
            nodes.insert(
                n,
                Node {
                    seq: entry.header.sequence,
                    parent,
                    name,
                    dir: entry.is_dir(),
                    created,
                },
            );
        }
    }
    Ok((nodes, corrupt))
}

/// Full path from parent references with sequence checks; `None` if the chain breaks.
fn path_of(
    n: u64,
    nodes: &HashMap<u64, Node>,
    memo: &mut HashMap<u64, Option<String>>,
) -> Option<String> {
    if let Some(p) = memo.get(&n) {
        return p.clone();
    }
    let mut chain = Vec::new();
    let mut cur = n;
    let result = loop {
        if cur == ROOT {
            break Some(String::new());
        }
        if chain.len() > 256 {
            break None; // loop guard
        }
        let node = nodes.get(&cur)?;
        let (pe, ps) = node.parent;
        if pe != ROOT && nodes.get(&pe).is_none_or(|p| p.seq != ps) {
            break None; // parent missing, deleted or reused
        }
        chain.push(node.name.clone());
        cur = pe;
    };
    let path = result.map(|mut p| {
        for seg in chain.iter().rev() {
            if !p.is_empty() {
                p.push('\\');
            }
            p.push_str(seg);
        }
        p
    });
    memo.insert(n, path.clone());
    path
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_guid(s: &str) -> bool {
    let s = s.trim_start_matches('{').trim_end_matches('}');
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && is_hex(p))
}

fn is_version(s: &str) -> bool {
    s.split('.').count() >= 3
        && s.split('.')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

const KEEP_USERS: [&str; 4] = ["PUBLIC", "DEFAULT", "DEFAULT USER", "ALL USERS"];

/// Upper-case and replace volatile parts: user names, SIDs, GUIDs, versions, long hex.
fn normalize(path: &str, enabled: bool) -> String {
    let segs: Vec<String> = path.split('\\').map(str::to_uppercase).collect();
    if !enabled {
        return segs.join("\\");
    }
    let mut out = Vec::with_capacity(segs.len());
    for (i, seg) in segs.iter().enumerate() {
        if i == 1 && segs[0] == "USERS" && !KEEP_USERS.contains(&seg.as_str()) {
            out.push("%USER%".to_string());
            continue;
        }
        if seg.starts_with("S-1-5-") {
            out.push("%SID%".to_string());
            continue;
        }
        // Each `_`/`.`-separated token that is a GUID, version or long hex becomes a placeholder.
        let norm: Vec<String> = seg
            .split('_')
            .map(|t| {
                let stem = t
                    .trim_end_matches(".MANIFEST")
                    .trim_end_matches(".CAT")
                    .trim_end_matches(".MUM");
                if is_guid(stem) {
                    t.replacen(stem, "%GUID%", 1)
                } else if is_version(stem) {
                    t.replacen(stem, "%VER%", 1)
                } else if stem.len() >= 16 && is_hex(stem) {
                    t.replacen(stem, "%HEX%", 1)
                } else {
                    t.to_string()
                }
            })
            .collect();
        out.push(norm.join("_"));
    }
    out.join("\\")
}

fn load_vwr(path: &str, normalized: bool) -> Result<HashSet<String>, Box<dyn Error>> {
    let text = std::fs::read_to_string(path)?;
    let mut set = HashSet::new();
    for line in text.lines().skip(1) {
        // Every field is quoted: "DirectoryName","Name","FullName",...
        let Some(full) = line.split("\",\"").nth(2) else {
            continue;
        };
        let Some(rel) = full.strip_prefix("C:\\") else {
            continue;
        };
        if rel.starts_with("PsExec_IgnoreThisFile") || rel == "test.csv" {
            continue;
        }
        set.insert(normalize(rel, normalized));
    }
    Ok(set)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let mft_path = args.next().ok_or("usage: s4-baseline <$MFT> <vwr.csv>")?;
    let vwr_path = args.next().ok_or("missing <vwr.csv>")?;

    let (nodes, corrupt) = load_mft(&mft_path)?;
    let mut memo = HashMap::new();
    let mut files: Vec<(String, i64)> = Vec::new();
    let (mut unknown, mut meta) = (0usize, 0usize);
    let mut numbers: Vec<u64> = nodes.keys().copied().collect();
    numbers.sort_unstable();
    for n in numbers {
        let Some(node) = nodes.get(&n) else { continue };
        if node.dir {
            continue;
        }
        if n < FIRST_USER_RECORD {
            meta += 1;
            continue;
        }
        match path_of(n, &nodes, &mut memo) {
            Some(p) if p.starts_with('$') => meta += 1,
            Some(p) => files.push((p, node.created)),
            None => unknown += 1,
        }
    }
    // Time window: the 7 days before the newest $SI creation time on the volume.
    let newest = files.iter().map(|f| f.1).max().unwrap_or(0);
    let window_start = newest - 7 * 86_400;
    let in_window = files.iter().filter(|f| f.1 >= window_start).count();
    println!(
        "[mft] in-use file records with a path: {} (metafiles {meta}, path unknown {unknown}, corrupt records {corrupt}); created in the last 7 days: {in_window}",
        files.len()
    );

    let pct = |a: usize, b: usize| {
        if b == 0 {
            0.0
        } else {
            100.0 * a as f64 / b as f64
        }
    };
    for normalized in [false, true] {
        let base = load_vwr(&vwr_path, normalized)?;
        let mut outside_by_area: BTreeMap<String, usize> = BTreeMap::new();
        let mut present = HashSet::new();
        let (mut outside, mut system, mut system_outside, mut window_outside) =
            (0usize, 0usize, 0usize, 0usize);
        for (f, created) in &files {
            let key = normalize(f, normalized);
            // Areas are always shown normalized, so no user names are printed.
            let shown = normalize(f, true);
            let is_user = shown.starts_with("USERS\\%USER%");
            if !is_user {
                system += 1;
            }
            if base.contains(&key) {
                present.insert(key);
                continue;
            }
            outside += 1;
            if !is_user {
                system_outside += 1;
            }
            if *created >= window_start {
                window_outside += 1;
            }
            let area: Vec<&str> = shown.split('\\').take(2).collect();
            let area = if area.len() == 2 {
                area.join("\\")
            } else {
                "(root)".into()
            };
            *outside_by_area.entry(area).or_default() += 1;
        }
        println!(
            "\n[{}] baseline paths {} | coverage of baseline {:.1} %",
            if normalized { "normalized" } else { "raw" },
            base.len(),
            pct(present.len(), base.len())
        );
        println!(
            "    all files          : outside {outside} / {} = {:.1} %",
            files.len(),
            pct(outside, files.len())
        );
        println!(
            "    excl. user profiles: outside {system_outside} / {system} = {:.1} %",
            pct(system_outside, system)
        );
        println!(
            "    last 7 days        : outside {window_outside} = {:.2} % of all files",
            pct(window_outside, files.len())
        );
        let mut top: Vec<(String, usize)> = outside_by_area.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (area, n) in top.iter().take(8) {
            println!("    {n:>7}  {:>5.1} %  {area}", pct(*n, files.len()));
        }
    }
    Ok(())
}
