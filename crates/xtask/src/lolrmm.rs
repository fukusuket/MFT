//! `xtask lolrmm <rmm_tools.json> <out-dir>`: one Sigma rule per LOLRMM tool (ADR 0017).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// The LOLRMM commit the rules are generated from, and its date (the rules' `date`).
const COMMIT: &str = "dc6ebe934bce";
const DATE: &str = "2026-10-06";

/// Writes the rules generated from the LOLRMM `rmm_tools.json` at `json` into `out`, replacing
/// any `lolrmm_*.yml` already there.
pub(crate) fn run(json: &str, out: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(json).map_err(|e| format!("reading {json}: {e}"))?;
    let rules = rules(&text)?;
    // A tool renamed or removed upstream must not leave its old rule behind.
    let entries = std::fs::read_dir(out).map_err(|e| format!("reading {out}: {e}"))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("reading {out}: {e}"))?.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.starts_with("lolrmm_") && name.ends_with(".yml") {
            std::fs::remove_file(&path).map_err(|e| format!("removing {}: {e}", path.display()))?;
        }
    }
    for (file, rule) in rules {
        let path = std::path::Path::new(out).join(&file);
        std::fs::write(&path, rule).map_err(|e| format!("writing {}: {e}", path.display()))?;
    }
    Ok(())
}

/// One `(file name, rule)` per tool that has a usable Windows path, sorted by file name.
fn rules(json: &str) -> Result<Vec<(String, String)>, String> {
    let tools: Vec<Value> = serde_json::from_str(json).map_err(|e| format!("reading JSON: {e}"))?;
    let mut by_name: BTreeMap<&str, (&str, BTreeSet<String>)> = BTreeMap::new();
    for tool in &tools {
        let name = plain(tool["Name"].as_str().ok_or("a tool without a Name")?)?;
        let category = plain(tool["Category"].as_str().unwrap_or("remote access tool"))?;
        let found = &mut by_name.entry(name).or_insert((category, BTreeSet::new())).1;
        for raw in raw_paths(tool) {
            found.extend(patterns(plain(raw)?));
        }
    }
    let mut out: BTreeMap<String, (&str, String)> = BTreeMap::new();
    for (name, (category, found)) in by_name {
        if found.is_empty() {
            continue;
        }
        let slug = slug(name);
        if slug.is_empty() {
            return Err(format!("no file name for tool {name:?}"));
        }
        let file = format!("lolrmm_{slug}.yml");
        if let Some((other, _)) = out.get(&file) {
            return Err(format!("tools {other:?} and {name:?} both map to {file}"));
        }
        out.insert(file, (name, rule(name, category, &found)));
    }
    Ok(out
        .into_iter()
        .map(|(file, (_, rule))| (file, rule))
        .collect())
}

/// Text that goes into a rule; a control character could end the YAML scalar early.
fn plain(text: &str) -> Result<&str, String> {
    if text.chars().any(char::is_control) {
        return Err(format!("control character in {text:?}"));
    }
    Ok(text)
}

/// Install paths, PE file names and Windows disk artifacts of one tool.
fn raw_paths(tool: &Value) -> Vec<&str> {
    let details = &tool["Details"];
    let installed = items(&details["InstallationPaths"])
        .iter()
        .filter_map(Value::as_str);
    let pe = items(&details["PEMetadata"])
        .iter()
        .filter_map(|m| m["Filename"].as_str());
    let disk = items(&tool["Artifacts"]["Disk"])
        .iter()
        .filter(|a| {
            a["OS"]
                .as_str()
                .is_some_and(|os| os.eq_ignore_ascii_case("windows"))
        })
        .filter_map(|a| a["File"].as_str());
    installed.chain(pe).chain(disk).collect()
}

/// The elements of a JSON array; none for anything else.
fn items(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

fn slug(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    words.join("_")
}

/// A YAML single-quoted scalar: no escapes except a doubled `'`.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// A UUID (version 8) from a 128-bit FNV-1a hash of the tool name: stable across refreshes.
fn rule_id(name: &str) -> String {
    let mut hash: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for byte in "lolrmm:".bytes().chain(name.bytes()) {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    hash = (hash & !(0xf << 76)) | (0x8 << 76); // version 8
    hash = (hash & !(0x3 << 62)) | (0x2 << 62); // RFC 9562 variant
    let hex = format!("{hash:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

fn rule(name: &str, category: &str, found: &BTreeSet<String>) -> String {
    let mut out = format!(
        "# Derived from LOLRMM (Apache-2.0, commit {COMMIT}). Detection Rule License (DRL) 1.1.\n\
         title: {title}\n\
         id: {id}\n\
         status: test\n\
         description: {description}\n\
         references:\n  - https://github.com/magicsword-io/LOLRMM\n\
         author: LOLRMM Project (generated by tool)\n\
         date: {DATE}\n\
         logsource:\n  product: windows\n  category: file_event\n\
         detection:\n",
        title = quoted(&format!("{name} file (LOLRMM)")),
        id = rule_id(name),
        description = quoted(&format!(
            "A file of {name}, a remote access tool ({category}) listed by LOLRMM."
        )),
    );
    out.push_str(&detection(found));
    out.push_str("  condition: 1 of them\nlevel: medium\n");
    out
}

/// Selections grouped by where a value's wildcards are. The engine prefilters rules by literal
/// text, and a glob with an inner wildcard gives it none, which made every event evaluate the
/// rule; such a glob gets its longest literal piece as an extra `contains`.
fn detection(found: &BTreeSet<String>) -> String {
    let wild = |c: char| c == '*' || c == '?';
    let (mut exact, mut starting, mut ending, mut containing) = (vec![], vec![], vec![], vec![]);
    let mut globs = vec![];
    for value in found {
        let inner = value.trim_start_matches('*').trim_end_matches('*');
        let (lead, trail) = (value.starts_with('*'), value.ends_with('*'));
        match (inner.contains(wild), lead, trail) {
            (false, false, false) => exact.push(inner),
            (false, false, true) => starting.push(inner),
            (false, true, false) => ending.push(inner),
            (false, true, true) => containing.push(inner),
            (true, ..) => globs.push(value.as_str()),
        }
    }
    let mut out = String::new();
    for (name, key, values) in [
        ("exact", "TargetFilename", exact),
        ("starting", "TargetFilename|startswith", starting),
        ("ending", "TargetFilename|endswith", ending),
        ("containing", "TargetFilename|contains", containing),
    ] {
        if !values.is_empty() {
            out.push_str(&format!("  {name}:\n    {key}:\n"));
            for value in values {
                out.push_str(&format!("      - {}\n", quoted(value)));
            }
        }
    }
    for (n, glob) in globs.iter().enumerate() {
        let piece = glob.split(wild).max_by_key(|p| p.len()).unwrap_or_default();
        out.push_str(&format!(
            "  glob{}:\n    TargetFilename: {}\n    TargetFilename|contains: {}\n",
            n + 1,
            quoted(glob),
            quoted(piece)
        ));
    }
    out
}

/// Sigma values for one LOLRMM path or file name; empty when it is not usable on Windows.
fn patterns(raw: &str) -> Vec<String> {
    let collapsed = collapse_backslashes(raw.trim());
    let raw = collapsed.as_str();
    if raw.contains('/') || raw.contains(r"\.") || raw.contains(".*") {
        return Vec::new(); // a Unix path, or a regular expression rather than a path
    }
    let raw = any_profile(&placeholders_to_wildcards(raw));
    let path = if raw.ends_with('\\') {
        format!("{raw}*") // a folder: anything in it
    } else if raw.contains('\\') {
        raw
    } else if raw.contains('.') {
        format!("*\\{raw}") // a bare file name: any folder
    } else {
        return Vec::new(); // a bare word such as a service name is not a file
    };
    if !names_something(&path) {
        return Vec::new();
    }
    let mut out: Vec<String> = program_files_variants(&path)
        .iter()
        .map(|p| sigma_escape(p))
        .collect();
    out.sort();
    out
}

/// Folders every Windows install has; a pattern made only of these matches almost anything.
const GENERIC_FOLDERS: [&str; 16] = [
    "program files",
    "program files (x86)",
    "programdata",
    "users",
    "appdata",
    "local",
    "locallow",
    "roaming",
    "windows",
    "system32",
    "syswow64",
    "temp",
    "microsoft",
    "start menu",
    "programs",
    "startup",
];

/// File names that, without a telling folder, name no tool: Windows' own files (which LOLRMM
/// lists for built-in remote access or masquerading) and generic names many products use.
const GENERIC_FILES: [&str; 33] = [
    "agent.exe",
    "agent32.exe",
    "agent64.exe",
    "appcore.exe",
    "autoupdate.exe",
    "avcore.exe",
    "closeapp.exe",
    "conf.json",
    "connect.exe",
    "cpuchk.exe",
    "dwm.exe",
    "era.exe",
    "installcore.exe",
    "installer.zip",
    "libeay32.dll",
    "mstsc.exe",
    "notificationhelper.exe",
    "quickassist.exe",
    "rdp.exe",
    "recycler.exe",
    "regapps.exe",
    "rmm.exe",
    "sciter.dll",
    "selfupdater.exe",
    "serviceconfig.xml",
    "setup.exe",
    "ssleay32.dll",
    "svchost.exe",
    "sysdiag.exe",
    "termsrv.exe",
    "update-shim.exe",
    "upload.exe",
    "wintun.dll",
];

fn alphanumerics(text: &str) -> usize {
    text.chars().filter(char::is_ascii_alphanumeric).count()
}

/// Whether the pattern points at one tool: a folder that is not a Windows default, or else a
/// file name that is neither common nor mostly wildcard.
fn names_something(path: &str) -> bool {
    let (folders, file) = path.rsplit_once('\\').unwrap_or(("", path));
    let telling_folder = folders.split('\\').any(|folder| {
        let literal = folder.replace(['*', '?'], "");
        alphanumerics(&literal) >= 3
            && !literal.ends_with(':')
            && !GENERIC_FOLDERS.contains(&literal.to_ascii_lowercase().as_str())
    });
    if telling_folder {
        return true;
    }
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    if file.contains(['*', '?']) {
        alphanumerics(stem) >= 6
    } else {
        alphanumerics(stem) >= 3 && !GENERIC_FILES.contains(&file.to_ascii_lowercase().as_str())
    }
}

/// `%TEMP%`, `<date>`, `[GUID]` and `(Random)` stand for unknown text: a wildcard. A `%` that
/// does not open an environment-variable name (as in `Status%4Operational.evtx`) is text.
fn placeholders_to_wildcards(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(c) = rest.chars().next() {
        let close = match c {
            '<' => Some('>'),
            '[' => Some(']'),
            '%' => Some('%'),
            _ => None,
        };
        let placeholder = close.and_then(|close| {
            let len = rest[1..].find(close)?;
            let inside = &rest[1..=len];
            let env_name = inside
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '(' | ')'));
            (c != '%' || (!inside.is_empty() && env_name)).then_some(len + 2)
        });
        match placeholder {
            Some(len) => {
                out.push('*');
                rest = &rest[len..];
            }
            None => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    replace_all_ignore_case(&out, "(random)", "*")
}

fn replace_all_ignore_case(text: &str, from: &str, to: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::new();
    let mut last = 0;
    for (at, _) in lower.match_indices(from) {
        out.push_str(&text[last..at]);
        out.push_str(to);
        last = at + from.len();
    }
    out.push_str(&text[last..]);
    out
}

/// Profiles every install has; any other name under `C:\Users` is one person's profile.
const SHARED_PROFILES: [&str; 4] = ["public", "default", "default user", "all users"];

/// A user name in the data (such as LOLRMM's test-VM user) is an example: the tool can be in any profile, and a
/// real user name must not end up in a rule.
fn any_profile(path: &str) -> String {
    const USERS: &str = r"c:\users\";
    if !path.to_ascii_lowercase().starts_with(USERS) {
        return path.to_string();
    }
    let rest = &path[USERS.len()..];
    let shared = |profile: &str| SHARED_PROFILES.contains(&profile.to_ascii_lowercase().as_str());
    match rest.split_once('\\') {
        Some((profile, below)) if shared(profile) => format!(r"C:\Users\{profile}\{below}"),
        Some((_, below)) => format!(r"C:\Users\*\{below}"),
        None if shared(rest) => format!(r"C:\Users\{rest}"),
        None => r"C:\Users\*".to_string(),
    }
}

const PROGRAM_FILES: [&str; 2] = [r"C:\Program Files (x86)\", r"C:\Program Files\"];

/// 32- and 64-bit installs differ only in this folder, and LOLRMM usually lists one.
fn program_files_variants(path: &str) -> Vec<String> {
    let lower = path.to_ascii_lowercase();
    for prefix in PROGRAM_FILES {
        if lower.starts_with(&prefix.to_ascii_lowercase()) {
            let rest = &path[prefix.len()..];
            return PROGRAM_FILES.iter().map(|p| format!("{p}{rest}")).collect();
        }
    }
    vec![path.to_string()]
}

/// Some LOLRMM paths are written with `\\` (JSON-escaped twice); a path never has an empty
/// segment, so a run of backslashes is one separator.
fn collapse_backslashes(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if !(c == '\\' && out.ends_with('\\')) {
            out.push(c);
        }
    }
    out
}

/// Sigma reads `\*` as a literal `*`; doubling every `\` keeps `*` and `?` wildcards.
fn sigma_escape(path: &str) -> String {
    path.replace('\\', "\\\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_backslashes_so_wildcards_stay_wildcards() {
        assert_eq!(
            patterns(r"C:\Users\*\AppData\Local\RemSupp\x.exe"),
            [r"C:\\Users\\*\\AppData\\Local\\RemSupp\\x.exe"]
        );
    }

    #[test]
    fn bare_name_with_extension_becomes_any_folder() {
        assert_eq!(patterns("AnyDesk.exe"), [r"*\\AnyDesk.exe"]);
        assert_eq!(patterns("teamviewerhost"), Vec::<String>::new());
    }

    #[test]
    fn program_files_yields_both_variants() {
        let both = [
            r"C:\\Program Files (x86)\\Splashtop\\*",
            r"C:\\Program Files\\Splashtop\\*",
        ];
        assert_eq!(patterns(r"C:\Program Files (x86)\Splashtop\*"), both);
        assert_eq!(patterns(r"c:\program files\Splashtop\*").len(), 2);
    }

    #[test]
    fn env_vars_and_placeholders_become_wildcards() {
        assert_eq!(
            patterns(r"%TEMP%\NiniteDownloads\*"),
            [r"*\\NiniteDownloads\\*"]
        );
        assert_eq!(
            patterns(r"C:\ProgramData\Getscreen.me\<date>.log"),
            [r"C:\\ProgramData\\Getscreen.me\\*.log"]
        );
        assert_eq!(
            patterns(r"C:\ScreenConnect Client (Random)\x.exe"),
            [r"C:\\ScreenConnect Client *\\x.exe"]
        );
    }

    #[test]
    fn drops_non_windows_regex_and_catch_all_patterns() {
        for raw in [
            "/opt/komari/*",
            " ~/opt/kaseya/*/logs*",
            "helpwire-operator/bin/helpwire-operator",
            r"TeamViewer\d\d_Logfile\.log",
            "*",
            r"*\*",
            r"C:\*",
            r"C:\Users\*\AppData\Local\Temp\*",
            r"%ProgramFiles%\*",
        ] {
            assert_eq!(patterns(raw), Vec::<String>::new(), "{raw}");
        }
        assert_eq!(patterns(r"  C:\Kaseya\x.exe "), [r"C:\\Kaseya\\x.exe"]);
    }

    #[test]
    fn trailing_backslash_means_folder_contents() {
        assert_eq!(
            patterns(r"C:\Program Files\TeamViewer\"),
            [
                r"C:\\Program Files (x86)\\TeamViewer\\*",
                r"C:\\Program Files\\TeamViewer\\*"
            ]
        );
    }

    #[test]
    fn drops_file_names_too_common_to_name_a_tool() {
        for raw in [
            "svchost.exe",
            "agent.exe",
            "Setup.exe",
            "rd.exe",
            "quickassist.exe",
            "*Agent.exe",
            "agent-*.exe",
            r"C:\Windows\*.exe",
            r"C:\Users\*\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\svchost.exe",
        ] {
            assert_eq!(patterns(raw), Vec::<String>::new(), "{raw}");
        }
        assert_eq!(patterns("remcos*.exe"), [r"*\\remcos*.exe"]);
        assert_eq!(
            patterns(r"C:\Users\*\AppData\Roaming\Microsoft\DeviceSync\svchost.exe"),
            [r"C:\\Users\\*\\AppData\\Roaming\\Microsoft\\DeviceSync\\svchost.exe"]
        );
    }

    #[test]
    fn doubled_backslashes_in_the_data_mean_one() {
        assert_eq!(
            patterns(r"C:\\ProgramData\\AMMYY\\access.log"),
            patterns(r"C:\ProgramData\AMMYY\access.log")
        );
        assert_eq!(
            patterns(r"C:\\ProgramData\\AMMYY\\access.log"),
            [r"C:\\ProgramData\\AMMYY\\access.log"]
        );
    }

    #[test]
    fn brackets_and_every_random_are_placeholders_but_a_lone_percent_is_text() {
        assert_eq!(
            patterns(r"C:\Devolutions\RemoteDesktopManager[GUID]\Mru.xml"),
            [r"C:\\Devolutions\\RemoteDesktopManager*\\Mru.xml"]
        );
        assert_eq!(
            patterns(r"C:\Foo (Random)\Bar (random)\x.exe"),
            [r"C:\\Foo *\\Bar *\\x.exe"]
        );
        assert_eq!(
            patterns(r"C:\Logs\Splashtop-Status%4Operational.evtx"),
            [r"C:\\Logs\\Splashtop-Status%4Operational.evtx"]
        );
        assert_eq!(
            patterns(r"C:\a%4b\%TEMP%\Ninite\x.exe"),
            [r"C:\\a%4b\\*\\Ninite\\x.exe"]
        );
    }

    #[test]
    fn a_named_user_profile_means_any_profile() {
        assert_eq!(
            patterns(r"C:\Users\alice\Downloads\WinSCP-Portable\"),
            [r"C:\\Users\\*\\Downloads\\WinSCP-Portable\\*"]
        );
        assert_eq!(
            patterns(r"c:\users\USERNAME\AppData\Roaming\Insync\Insync.exe"),
            [r"C:\\Users\\*\\AppData\\Roaming\\Insync\\Insync.exe"]
        );
        assert_eq!(
            patterns(r"C:\Users\Public\Foo\x.exe"),
            [r"C:\\Users\\Public\\Foo\\x.exe"]
        );
    }

    const TWO_TOOLS: &str = r#"[
        {"Name": "Foo Remote", "Category": "RMM",
         "Details": {"InstallationPaths": ["C:\\Foo\\foo.exe", "foo.exe"],
                     "PEMetadata": [{"Filename": "foo-agent.exe"}]},
         "Artifacts": {"Disk": [{"File": "C:\\ProgramData\\Foo\\log.txt", "OS": "Windows"},
                                {"File": "/var/log/foo.log", "OS": "Linux"}]}},
        {"Name": "Foo Remote", "Category": "RMM",
         "Details": {"InstallationPaths": ["foo.exe", "C:\\Bar\\foo.exe"]}},
        {"Name": "Nothing", "Category": "RAT",
         "Details": {"InstallationPaths": ["nothing", "/opt/nothing"]}}
    ]"#;

    #[test]
    fn one_rule_per_tool_merging_duplicate_names_sorted() -> Result<(), String> {
        let rules = rules(TWO_TOOLS)?;

        let names: Vec<&str> = rules.iter().map(|(file, _)| file.as_str()).collect();
        assert_eq!(names, ["lolrmm_foo_remote.yml"]);
        assert!(
            rules[0].1.contains(concat!(
                "  exact:\n",
                "    TargetFilename:\n",
                "      - 'C:\\\\Bar\\\\foo.exe'\n",
                "      - 'C:\\\\Foo\\\\foo.exe'\n",
                "      - 'C:\\\\ProgramData\\\\Foo\\\\log.txt'\n",
                "  ending:\n",
                "    TargetFilename|endswith:\n",
                "      - '\\\\foo-agent.exe'\n",
                "      - '\\\\foo.exe'\n",
                "  condition: 1 of them\n",
            )),
            "{}",
            rules[0].1
        );
        Ok(())
    }

    #[test]
    fn values_are_grouped_by_where_their_wildcards_are() -> Result<(), String> {
        let json = r#"[{"Name": "Foo", "Details": {"InstallationPaths": [
            "C:\\Foo\\x.exe",
            "C:\\Program Files\\Foo\\",
            "foo-agent.exe",
            "%APPDATA%\\Foo\\*",
            "C:\\Users\\*\\AppData\\Roaming\\Foo\\ad_*.trace"
        ]}}]"#;

        let rules = rules(json)?;

        let detection = rules[0].1.split("detection:\n").nth(1).unwrap_or_default();
        assert_eq!(
            detection,
            concat!(
                "  exact:\n",
                "    TargetFilename:\n",
                "      - 'C:\\\\Foo\\\\x.exe'\n",
                "  starting:\n",
                "    TargetFilename|startswith:\n",
                "      - 'C:\\\\Program Files (x86)\\\\Foo\\\\'\n",
                "      - 'C:\\\\Program Files\\\\Foo\\\\'\n",
                "  ending:\n",
                "    TargetFilename|endswith:\n",
                "      - '\\\\foo-agent.exe'\n",
                "  containing:\n",
                "    TargetFilename|contains:\n",
                "      - '\\\\Foo\\\\'\n",
                "  glob1:\n",
                "    TargetFilename: 'C:\\\\Users\\\\*\\\\AppData\\\\Roaming\\\\Foo\\\\ad_*.trace'\n",
                "    TargetFilename|contains: '\\\\AppData\\\\Roaming\\\\Foo\\\\ad_'\n",
                "  condition: 1 of them\n",
                "level: medium\n",
            )
        );
        Ok(())
    }

    #[test]
    fn names_that_share_a_file_name_or_have_none_are_errors() {
        let tools = |a: &str, b: &str| {
            format!(
                r#"[{{"Name": "{a}", "Details": {{"InstallationPaths": ["alpha.exe"]}}}},
                    {{"Name": "{b}", "Details": {{"InstallationPaths": ["bravo.exe"]}}}}]"#
            )
        };
        assert!(rules(&tools("Foo Bar", "Foo-Bar")).is_err());
        assert!(rules(&tools("Foo", "++")).is_err());
        assert!(rules(&tools("Foo", "Bar")).is_ok());
    }

    #[test]
    fn run_replaces_only_the_generated_rules() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("xtask-lolrmm-{}", std::process::id()));
        let out = dir.join("rules");
        std::fs::create_dir_all(&out)?;
        let json = dir.join("rmm_tools.json");
        std::fs::write(&json, TWO_TOOLS)?;
        std::fs::write(out.join("lolrmm_renamed_upstream.yml"), "stale")?;
        std::fs::write(out.join("hand_written.yml"), "keep")?;

        run(&json.to_string_lossy(), &out.to_string_lossy())?;

        let mut names: Vec<String> = std::fs::read_dir(&out)?
            .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect::<Result<_, _>>()?;
        names.sort();
        assert_eq!(names, ["hand_written.yml", "lolrmm_foo_remote.yml"]);
        Ok(())
    }

    fn id_of(rule: &str) -> Option<&str> {
        rule.lines().find_map(|line| line.strip_prefix("id: "))
    }

    #[test]
    fn rule_id_is_stable_and_uuid_shaped() -> Result<(), String> {
        let one = rules(TWO_TOOLS)?;
        let alone =
            rules(r#"[{"Name": "Foo Remote", "Details": {"InstallationPaths": ["alpha.exe"]}}]"#)?;
        let other =
            rules(r#"[{"Name": "Bar Remote", "Details": {"InstallationPaths": ["alpha.exe"]}}]"#)?;

        let id = id_of(&one[0].1).ok_or("no id")?;
        assert_eq!(Some(id), id_of(&alone[0].1));
        assert_ne!(Some(id), id_of(&other[0].1));
        let groups: Vec<usize> = id.split('-').map(str::len).collect();
        assert_eq!(groups, [8, 4, 4, 4, 12], "{id}");
        assert!(
            id.bytes().all(|b| b == b'-' || b.is_ascii_hexdigit()),
            "{id}"
        );
        assert_eq!(&id[14..15], "8", "version 8: {id}");
        Ok(())
    }

    #[test]
    fn same_json_gives_byte_identical_rules_in_any_input_order() -> Result<(), String> {
        let mut tools: Vec<Value> = serde_json::from_str(TWO_TOOLS).map_err(|e| e.to_string())?;
        tools.push(
            serde_json::json!({"Name": "Bar", "Details": {"InstallationPaths": ["bar.exe"]}}),
        );
        let forward = serde_json::to_string(&tools).map_err(|e| e.to_string())?;
        tools.reverse();
        let backward = serde_json::to_string(&tools).map_err(|e| e.to_string())?;

        assert_eq!(rules(&forward)?, rules(&forward)?);
        assert_eq!(rules(&forward)?, rules(&backward)?);
        Ok(())
    }

    #[test]
    fn rejects_control_characters_in_rule_text() {
        for json in [
            r#"[{"Name": "Evil\nlevel: critical", "Details": {"InstallationPaths": ["alpha.exe"]}}]"#,
            r#"[{"Name": "Evil", "Category": "RMM\r", "Details": {"InstallationPaths": ["alpha.exe"]}}]"#,
            r#"[{"Name": "Evil", "Details": {"InstallationPaths": ["a\n.exe"]}}]"#,
        ] {
            assert!(rules(json).is_err(), "{json}");
        }
    }
}
