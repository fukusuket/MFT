//! NTFS facts to Sigma events. Exposes facts only; the rules decide (AGENTS.md).

use baseline::Status;
use mft_parse::Entry;
use resolve::Resolution;
use sigma::Event;
use usn_parse::UsnEvent;

/// The `file_event` for a file, or `None` when there is nothing to match on.
pub fn file_event(
    entry: &Entry,
    resolution: &Resolution<'_>,
    baseline: Option<Status>,
) -> Option<Event> {
    let is_root = matches!(resolution, Resolution::Resolved(segments) if segments.is_empty());
    if entry.is_dir || is_root {
        return None; // file_event is about files
    }
    // Phase 1 inputs are system volumes, so the drive is C: (P1-4 H1).
    let target = format!("C:{}", resolution.path_text()?);
    let mut fields = vec![("TargetFilename", target)];
    fields.extend(entry.si_created.map(|t| ("CreationUtcTime", t.sysmon())));
    fields.extend(
        resolve::chosen_name(&entry.names).map(|n| ("FnCreationUtcTime", n.created.sysmon())),
    );
    Some(Event {
        product: "windows",
        category: "file_event",
        // "Outside baseline" is a logsource, not a field (architecture.md).
        service: (baseline == Some(Status::Outside)).then_some("baseline_outside"),
        fields,
    })
}

/// The Sigma events for a USN record whose file had the path `target` (and `source` before a
/// rename), or none when there is nothing to match on.
pub fn usn_events(
    event: &UsnEvent,
    target: &str,
    source: Option<&str>,
    baseline: Option<Status>,
) -> Vec<Event> {
    let _ = baseline;
    // The closing record carries every reason of the handle: one event per operation.
    if event.reason & CLOSE == 0 {
        return Vec::new();
    }
    let time = event.time.sysmon();
    [
        (FILE_CREATE, "file_event"),
        (RENAME_NEW_NAME, "file_rename"),
        (BASIC_INFO_CHANGE, "file_change"),
        (FILE_DELETE, "file_delete"),
    ]
    .into_iter()
    .filter(|&(flag, _)| event.reason & flag != 0)
    .map(|(_, category)| {
        let mut fields = Vec::new();
        if category == "file_rename" {
            fields.extend(source.map(|s| ("SourceFilename", format!("C:{s}"))));
        }
        fields.push(("TargetFilename", format!("C:{target}")));
        if category == "file_event" {
            fields.push(("CreationUtcTime", time.clone()));
        }
        fields.push(("UtcTime", time.clone()));
        Event {
            product: "windows",
            category,
            service: None,
            fields,
        }
    })
    .collect()
}

/// `USN_REASON_*` flags (winioctl.h).
const FILE_CREATE: u32 = 0x0000_0100;
const FILE_DELETE: u32 = 0x0000_0200;
const RENAME_NEW_NAME: u32 = 0x0000_2000;
const BASIC_INFO_CHANGE: u32 = 0x0000_8000;
const CLOSE: u32 = 0x8000_0000;

#[cfg(test)]
mod tests {
    use super::*;
    use mft_parse::{FileName, Namespace};
    use ntfs_types::{FileRef, Filetime, NtfsName};

    fn name(text: &str) -> NtfsName {
        NtfsName::from_units(&text.encode_utf16().collect::<Vec<_>>())
    }

    fn file(si: u64, fn_created: u64) -> Entry {
        Entry {
            file_ref: FileRef::from_raw(40),
            in_use: true,
            is_dir: false,
            base: None,
            si_created: Some(Filetime::from_raw(si)),
            names: vec![FileName {
                name: name("x.exe"),
                parent: FileRef::from_raw(5),
                namespace: Namespace::Win32,
                created: Filetime::from_raw(fn_created),
            }],
            diagnostics: vec![],
        }
    }

    fn usn(reason: u32) -> UsnEvent {
        UsnEvent {
            offset: 0,
            usn: 4096,
            file: FileRef::from_raw(40),
            parent: FileRef::from_raw(5),
            time: Filetime::from_raw(133_536_836_961_234_567),
            reason,
            attributes: 0x20, // FILE_ATTRIBUTE_ARCHIVE
            name: name("x.exe"),
        }
    }

    /// Category, service and fields of each event.
    type Shown = (
        &'static str,
        Option<&'static str>,
        Vec<(&'static str, String)>,
    );

    fn shown(events: Vec<Event>) -> Vec<Shown> {
        events
            .into_iter()
            .map(|e| (e.category, e.service, e.fields))
            .collect()
    }

    #[test]
    fn a_closed_usn_create_is_a_file_event_at_the_usn_time() {
        let events = usn_events(&usn(0x100 | CLOSE), r"\Windows\Temp\x.exe", None, None);

        assert_eq!(
            shown(events),
            [(
                "file_event",
                None,
                vec![
                    ("TargetFilename", r"C:\Windows\Temp\x.exe".to_string()),
                    ("CreationUtcTime", "2024-02-29 12:34:56.123".to_string()),
                    ("UtcTime", "2024-02-29 12:34:56.123".to_string()),
                ]
            )]
        );
    }

    #[test]
    fn records_before_the_close_are_not_events() {
        assert!(usn_events(&usn(0x100), r"\x.exe", None, None).is_empty());
        assert!(usn_events(&usn(0x100 | 0x2), r"\x.exe", None, None).is_empty());
    }

    #[test]
    fn a_closed_usn_delete_is_a_file_delete() {
        let events = usn_events(&usn(0x200 | CLOSE), r"\x.evtx", None, None);

        assert_eq!(
            shown(events),
            [(
                "file_delete",
                None,
                vec![
                    ("TargetFilename", r"C:\x.evtx".to_string()),
                    ("UtcTime", "2024-02-29 12:34:56.123".to_string()),
                ]
            )]
        );
    }

    #[test]
    fn a_closed_usn_rename_is_a_file_rename_with_the_old_path_when_known() {
        let rename = |source| shown(usn_events(&usn(0x2000 | CLOSE), r"\a.exe", source, None));
        let time = ("UtcTime", "2024-02-29 12:34:56.123".to_string());
        let target = ("TargetFilename", r"C:\a.exe".to_string());

        assert_eq!(
            rename(Some(r"\a.txt")),
            [(
                "file_rename",
                None,
                vec![
                    ("SourceFilename", r"C:\a.txt".to_string()),
                    target.clone(),
                    time.clone()
                ]
            )]
        );
        assert_eq!(rename(None), [("file_rename", None, vec![target, time])]);
    }

    #[test]
    fn a_closed_basic_info_change_is_a_file_change_but_a_data_write_is_not() {
        let events = usn_events(&usn(0x8000 | CLOSE), r"\x.exe", None, None);

        assert_eq!(
            shown(events),
            [(
                "file_change",
                None,
                vec![
                    ("TargetFilename", r"C:\x.exe".to_string()),
                    ("UtcTime", "2024-02-29 12:34:56.123".to_string()),
                ]
            )]
        );
        assert!(usn_events(&usn(0x2 | CLOSE), r"\x.exe", None, None).is_empty());
    }

    #[test]
    fn one_record_gives_one_event_per_reason_in_a_fixed_order() {
        let all = 0x100 | 0x200 | 0x2000 | 0x8000 | CLOSE;
        let categories: Vec<&str> = usn_events(&usn(all), r"\x.exe", None, None)
            .iter()
            .map(|e| e.category)
            .collect();

        assert_eq!(
            categories,
            ["file_event", "file_rename", "file_change", "file_delete"]
        );
    }

    #[test]
    fn file_event_has_target_filename_with_drive_and_both_created_times() {
        let (windows, temp, exe) = (name("Windows"), name("Temp"), name("x.exe"));
        let resolution = Resolution::Resolved(vec![&windows, &temp, &exe]);

        let event = file_event(
            &file(133_444_555_666_777_888, 133_536_836_961_234_567),
            &resolution,
            None,
        );

        let event = event.map(|e| (e.product, e.category, e.service, e.fields));
        assert_eq!(
            event,
            Some((
                "windows",
                "file_event",
                None,
                vec![
                    ("TargetFilename", r"C:\Windows\Temp\x.exe".to_string()),
                    ("CreationUtcTime", "2023-11-14 17:12:46.677".to_string()),
                    ("FnCreationUtcTime", "2024-02-29 12:34:56.123".to_string()),
                ]
            ))
        );
    }

    #[test]
    fn outside_baseline_files_get_the_baseline_outside_service() {
        let exe = name("x.exe");
        let resolution = Resolution::Resolved(vec![&exe]);
        let service = |status| file_event(&file(0, 0), &resolution, status).and_then(|e| e.service);

        assert_eq!(service(Some(Status::Outside)), Some("baseline_outside"));
        assert_eq!(service(Some(Status::Standard)), None);
        assert_eq!(service(None), None);
    }

    #[test]
    fn no_event_for_directories_unknown_paths_or_the_root() {
        let dir_name = name("Temp");
        let mut dir = file(0, 0);
        dir.is_dir = true;

        assert!(file_event(&dir, &Resolution::Resolved(vec![&dir_name]), None).is_none());
        assert!(file_event(&file(0, 0), &Resolution::Unknown, None).is_none());
        assert!(file_event(&file(0, 0), &Resolution::Resolved(vec![]), None).is_none());
    }
}
