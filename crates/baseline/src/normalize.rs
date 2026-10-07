//! ADR 0012: upper-case each segment with `$UpCase`, then replace volatile parts with
//! NUL-prefixed placeholders. NUL can't occur in an NTFS name, so no real file matches one.

use ntfs_types::NormPath;

/// Bump when any rule changes: baseline files built with other rules are refused (ADR 0012).
pub(crate) const NORMALIZATION_VERSION: u32 = 2; // 2: rule 1 only for folders (ADR 0015)

const SEPARATOR: u16 = 0x5C; // backslash
const UNDERSCORE: u16 = 0x5F;
const HYPHEN: u16 = 0x2D;
const DOT: u16 = 0x2E;
const LEFT_BRACE: u16 = 0x7B;
const RIGHT_BRACE: u16 = 0x7D;

/// Kept outside a replaced token, as in the S4 spike (ADR 0012).
const KEPT_SUFFIXES: [&str; 3] = [".MANIFEST", ".CAT", ".MUM"];

/// Profile names under `USERS` that are the same on every install.
const SHARED_PROFILES: [&str; 4] = ["PUBLIC", "DEFAULT", "DEFAULT USER", "ALL USERS"];

/// The lookup key for a path given as segments from the root down.
pub(crate) fn key(segments: &[&[u16]]) -> Option<Vec<u16>> {
    // A separator or NUL inside a name would let crafted names collide with real keys.
    if segments
        .iter()
        .any(|s| s.contains(&SEPARATOR) || s.contains(&0))
    {
        return None;
    }
    let upper: Vec<NormPath> = segments.iter().map(|s| NormPath::from_units(s)).collect();
    let mut key = Vec::new();
    for (i, segment) in upper.iter().enumerate() {
        if i > 0 {
            key.push(SEPARATOR);
        }
        // Rule 1 names a profile folder, so the segment must have something below it (ADR 0015).
        let under_users = i == 1 && upper.len() > 2 && is(upper[0].units(), "USERS");
        key.extend(normalize_segment(segment.units(), under_users));
    }
    Some(key)
}

/// One upper-cased segment with rules 1 (user name) and 2 (SID) applied.
fn normalize_segment(units: &[u16], under_users: bool) -> Vec<u16> {
    if under_users && !SHARED_PROFILES.iter().any(|p| is(units, p)) {
        return placeholder("USER").collect();
    }
    if starts_with(units, "S-1-5-") {
        return placeholder("SID").collect();
    }
    let mut out = Vec::with_capacity(units.len());
    for (i, token) in units.split(|&u| u == UNDERSCORE).enumerate() {
        if i > 0 {
            out.push(UNDERSCORE);
        }
        let (stem, suffix) = token.split_at(token.len() - kept_suffix_len(token));
        if is_guid(stem) {
            out.extend(placeholder("GUID"));
        } else if is_version(stem) {
            out.extend(placeholder("VER"));
        } else if stem.len() >= 16 && stem.iter().all(|&u| is_hex_digit(u)) {
            out.extend(placeholder("HEX"));
        } else {
            out.extend_from_slice(stem);
        }
        out.extend_from_slice(suffix);
    }
    out
}

/// `8-4-4-4-12` hex digits, optionally in braces.
fn is_guid(stem: &[u16]) -> bool {
    let inner = stem.strip_prefix(&[LEFT_BRACE]).unwrap_or(stem);
    let inner = inner.strip_suffix(&[RIGHT_BRACE]).unwrap_or(inner);
    let groups: Vec<&[u16]> = inner.split(|&u| u == HYPHEN).collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.iter().all(|&u| is_hex_digit(u)))
}

/// Three or more dot-separated decimal numbers, e.g. `10.0.22621.1`.
fn is_version(stem: &[u16]) -> bool {
    stem.split(|&u| u == DOT).count() >= 3
        && stem
            .split(|&u| u == DOT)
            .all(|part| !part.is_empty() && part.iter().all(|&u| (0x30..=0x39).contains(&u)))
}

fn is_hex_digit(u: u16) -> bool {
    matches!(u, 0x30..=0x39 | 0x41..=0x46) // 0-9, A-F (segments are upper-cased)
}

fn is(units: &[u16], text: &str) -> bool {
    units.iter().copied().eq(text.encode_utf16())
}

/// Length of the `.MANIFEST`/`.CAT`/`.MUM` ending of `token`, or 0.
fn kept_suffix_len(token: &[u16]) -> usize {
    KEPT_SUFFIXES
        .iter()
        .map(|s| s.encode_utf16().collect::<Vec<u16>>())
        .find(|s| token.ends_with(s))
        .map_or(0, |s| s.len())
}

fn starts_with(units: &[u16], prefix: &str) -> bool {
    let prefix: Vec<u16> = prefix.encode_utf16().collect();
    units.starts_with(&prefix)
}

fn placeholder(tag: &str) -> impl Iterator<Item = u16> + '_ {
    std::iter::once(0).chain(tag.encode_utf16())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The key as text, with placeholders' NUL shown as `␀`.
    fn shown(path: &str) -> Option<String> {
        let segments: Vec<Vec<u16>> = path
            .split('\\')
            .map(|s| s.encode_utf16().collect())
            .collect();
        let refs: Vec<&[u16]> = segments.iter().map(Vec::as_slice).collect();
        key(&refs).map(|k| String::from_utf16_lossy(&k).replace('\0', "␀"))
    }

    #[test]
    fn upper_cases_with_upcase_and_joins_with_backslash() {
        assert_eq!(
            shown(r"Windows\System32\äbc.dll").as_deref(),
            Some(r"WINDOWS\SYSTEM32\ÄBC.DLL")
        );
    }

    #[test]
    fn rule_1_replaces_the_user_name_under_users() {
        let cases = [
            (r"Users\alice\NTUSER.DAT", r"USERS\␀USER\NTUSER.DAT"),
            (r"Users\Public\Desktop\x.lnk", r"USERS\PUBLIC\DESKTOP\X.LNK"),
            (r"Users\Default\NTUSER.DAT", r"USERS\DEFAULT\NTUSER.DAT"),
            (r"Users\Default User", r"USERS\DEFAULT USER"),
            (r"Users\All Users", r"USERS\ALL USERS"),
            (r"Users\Public\alice", r"USERS\PUBLIC\ALICE"),
            (r"Windows\alice", r"WINDOWS\ALICE"),
            (r"Users", r"USERS"),
        ];
        for (path, expected) in cases {
            assert_eq!(shown(path).as_deref(), Some(expected), "{path}");
        }
    }

    #[test]
    fn rule_2_replaces_sid_segments() {
        let cases = [
            (
                r"$Recycle.Bin\S-1-5-21-1-2-3-1001\$RABC.exe",
                r"$RECYCLE.BIN\␀SID\$RABC.EXE",
            ),
            (
                r"ProgramData\Microsoft\Crypto\RSA\S-1-5-18\x",
                r"PROGRAMDATA\MICROSOFT\CRYPTO\RSA\␀SID\X",
            ),
            (r"Windows\S-1-4-x", r"WINDOWS\S-1-4-X"),
        ];
        for (path, expected) in cases {
            assert_eq!(shown(path).as_deref(), Some(expected), "{path}");
        }
    }

    #[test]
    fn rule_3_replaces_guid_tokens() {
        let cases = [
            (
                r"ProgramData\{12345678-abcd-1234-1234-123456789ABC}",
                r"PROGRAMDATA\␀GUID",
            ),
            (
                r"x\12345678-abcd-1234-1234-123456789abc_data",
                r"X\␀GUID_DATA",
            ),
            (
                r"x\pkg_12345678-abcd-1234-1234-123456789abc.manifest",
                r"X\PKG_␀GUID.MANIFEST",
            ),
            (
                r"x\pkg_12345678-abcd-1234-1234-123456789abc.cat",
                r"X\PKG_␀GUID.CAT",
            ),
            (
                r"x\1234567-abcd-1234-1234-123456789abc",
                r"X\1234567-ABCD-1234-1234-123456789ABC",
            ),
            (
                r"x\12345678-abcd-1234-1234-123456789abg",
                r"X\12345678-ABCD-1234-1234-123456789ABG",
            ),
        ];
        for (path, expected) in cases {
            assert_eq!(shown(path).as_deref(), Some(expected), "{path}");
        }
    }

    #[test]
    fn rule_4_replaces_version_tokens() {
        let cases = [
            (
                r"Windows\WinSxS\amd64_x-y_10.0.22621.1_none",
                r"WINDOWS\WINSXS\AMD64_X-Y_␀VER_NONE",
            ),
            (
                r"Windows\servicing\Packages\Pkg~31bf~amd64~~10.0.1.2.mum",
                r"WINDOWS\SERVICING\PACKAGES\PKG~31BF~AMD64~~10.0.1.2.MUM",
            ),
            (r"x\a_1.2.3.mum", r"X\A_␀VER.MUM"),
            (r"x\a_1.2_b", r"X\A_1.2_B"),
            (r"x\a_1..3", r"X\A_1..3"),
            (r"x\a_1.2.x", r"X\A_1.2.X"),
        ];
        for (path, expected) in cases {
            assert_eq!(shown(path).as_deref(), Some(expected), "{path}");
        }
    }

    #[test]
    fn rule_5_replaces_long_hex_tokens() {
        let cases = [
            (
                r"Windows\WinSxS\amd64_x_31bf3856ad364e35_none_0123456789abcdef",
                r"WINDOWS\WINSXS\AMD64_X_␀HEX_NONE_␀HEX",
            ),
            (r"x\a_0123456789abcde", r"X\A_0123456789ABCDE"),
            (r"x\a_0123456789abcdeg", r"X\A_0123456789ABCDEG"),
        ];
        for (path, expected) in cases {
            assert_eq!(shown(path).as_deref(), Some(expected), "{path}");
        }
    }

    #[test]
    fn look_alikes_stay_literal_and_a_backslash_inside_a_name_gives_no_key() {
        assert_eq!(shown(r"Users\%USER%\x").as_deref(), Some(r"USERS\␀USER\X"));
        assert_eq!(
            shown(r"Windows\%USER%\x").as_deref(),
            Some(r"WINDOWS\%USER%\X")
        );
        assert_eq!(shown(r"x\%GUID%_%VER%").as_deref(), Some(r"X\%GUID%_%VER%"));

        let crafted: Vec<u16> = r"System32\cmd.exe".encode_utf16().collect();
        let windows: Vec<u16> = "Windows".encode_utf16().collect();
        assert_eq!(key(&[&windows, &crafted]), None);

        // NTFS forbids NUL, but crafted records can carry it; it must not forge a placeholder.
        let forged: Vec<u16> = "\0GUID".encode_utf16().collect();
        assert_eq!(key(&[&windows, &forged]), None);
    }

    #[test]
    fn rule_1_leaves_files_directly_under_users_literal() {
        assert_eq!(
            shown(r"Users\desktop.ini").as_deref(),
            Some(r"USERS\DESKTOP.INI")
        );
        assert_eq!(shown(r"Users\evil.exe").as_deref(), Some(r"USERS\EVIL.EXE"));
        assert_eq!(
            shown(r"Users\bob\evil.exe").as_deref(),
            Some(r"USERS\␀USER\EVIL.EXE")
        );
    }

    use proptest::prelude::*;

    fn segment() -> impl Strategy<Value = Vec<u16>> {
        let unit = prop_oneof![
            4 => proptest::sample::select(b"Users_.-{}S15abcF09\\\0".map(u16::from).to_vec()),
            1 => any::<u16>(),
        ];
        proptest::collection::vec(unit, 0..40)
    }

    proptest! {
        /// Any segments: no panic; a key exists exactly when no segment has `\` or NUL;
        /// the key ignores ASCII case.
        #[test]
        fn any_segments(segments in proptest::collection::vec(segment(), 0..6)) {
            let refs: Vec<&[u16]> = segments.iter().map(Vec::as_slice).collect();
            let k = key(&refs);
            let clean = segments.iter().all(|s| !s.contains(&SEPARATOR) && !s.contains(&0));
            prop_assert_eq!(k.is_some(), clean);

            let lower: Vec<Vec<u16>> = segments
                .iter()
                .map(|s| s.iter().map(|&u| if (0x41..=0x5A).contains(&u) { u + 0x20 } else { u }).collect())
                .collect();
            let lower_refs: Vec<&[u16]> = lower.iter().map(Vec::as_slice).collect();
            prop_assert_eq!(key(&lower_refs), k);
        }
    }
}
