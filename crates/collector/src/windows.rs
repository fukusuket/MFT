//! Reads `$MFT`, `$J`, `$Boot` and `$SDS` from a live volume with `ntfs-reader` (ADR 0005).

use std::ffi::OsStr;
use std::io::Write;

use ntfs_reader::{ExtentLocation, Mft, NtfsReaderError, Volume};
use ntfs_types::Filetime;

use crate::{Collection, Extent, ExtentKind, FileSize, Meta, unstored};

const MFT: u64 = 0;
const BOOT: u64 = 7;
const SECURE: u64 = 9;
const EXTEND: u64 = 11;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("access to {0} denied; run as administrator")]
    AccessDenied(String),
    #[error("reading {volume}: {source}")]
    Ntfs {
        volume: String,
        source: NtfsReaderError,
    },
    #[error(
        "{volume}: {bytes} bytes of {name} are not stored on the volume; not collected with a gap"
    )]
    Unstored {
        volume: String,
        name: &'static str,
        bytes: u64,
    },
    #[error("{volume} has no MFT record {record}")]
    MissingRecord { volume: String, record: u64 },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Writes the zip of `volume` (e.g. `C:`) to `writer` and returns it with the `meta.json`
/// written. `tool` and `host` go into `meta.json`; `now` is read once, when the copy ends.
/// A volume without a USN journal gives no `$J` entry and `j: None`.
pub fn collect<W: Write>(
    volume: &str,
    writer: W,
    (tool, host): (String, String),
    started: Filetime,
    now: impl FnOnce() -> Filetime,
) -> Result<(W, Meta), Error> {
    let path = format!(r"\\.\{volume}");
    let ntfs = |source| match source {
        NtfsReaderError::AccessDenied => Error::AccessDenied(path.clone()),
        source => Error::Ntfs {
            volume: path.clone(),
            source,
        },
    };
    let mft = Volume::new(&path).and_then(Mft::new).map_err(ntfs)?;
    let record = |record| {
        mft.record(record).ok_or_else(|| Error::MissingRecord {
            volume: path.clone(),
            record,
        })
    };
    let journal = mft.files().find(|file| {
        file.names().any(|name| {
            name.parent_number() == EXTEND && name.to_os_string() == OsStr::new("$UsnJrnl")
        })
    });
    let streams = [
        ("$MFT", Some(record(MFT)?), None),
        ("$J", journal, Some("$J")),
        ("$Boot", Some(record(BOOT)?), None),
        ("$SDS", Some(record(SECURE)?), Some("$SDS")),
    ];
    let mut collection = Collection::new(writer, started);
    let (mut files, mut j) = (Vec::new(), None);
    for (name, file, stream) in streams {
        let Some(file) = file else { continue };
        let mut reader = match file.open_stream(stream.map(OsStr::new)) {
            // `$UsnJrnl` without `$J`: the journal is being deleted; same as no journal.
            Err(NtfsReaderError::StreamNotFound { .. }) if name == "$J" => continue,
            opened => opened.map_err(ntfs)?,
        };
        let extents: Vec<_> = (reader.extents().iter())
            .map(|extent| Extent {
                offset: extent.stream_offset,
                length: extent.length,
                kind: match extent.location {
                    ExtentLocation::Resident | ExtentLocation::Volume { .. } => ExtentKind::Stored,
                    ExtentLocation::Sparse => ExtentKind::Sparse,
                    _ => ExtentKind::Lost,
                },
            })
            .collect();
        let bytes = unstored(&extents);
        if name != "$J" && bytes > 0 {
            return Err(Error::Unstored {
                volume: path,
                name,
                bytes,
            });
        }
        let counts = collection.add(name, &mut reader, &extents)?;
        files.push(FileSize {
            name: name.to_owned(),
            size: counts.stored,
        });
        if name == "$J" {
            j = Some(counts);
        }
    }
    let meta = Meta {
        tool,
        host,
        volume: volume.to_owned(),
        started,
        finished: now(),
        files,
        j,
    };
    let writer = collection.finish(&meta)?;
    Ok((writer, meta))
}
