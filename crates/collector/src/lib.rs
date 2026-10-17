//! Copies the NTFS metadata files of a live volume into one zip (ADR 0005, ADR 0024).

use std::io::{Read, Seek, SeekFrom, Write};

use flate2::Compression;
use flate2::write::DeflateEncoder;
use ntfs_types::Filetime;
use rawzip::time::UtcDateTime;
use rawzip::{CompressionMethod, ZipArchiveWriter};
use serde::{Serialize, Serializer};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{Error, collect};

/// A stretch of a stream and where its bytes are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent {
    /// Offset of the stretch from the start of the stream.
    pub offset: u64,
    pub length: u64,
    pub kind: ExtentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtentKind {
    /// On the volume or in the MFT record.
    Stored,
    /// Not stored; reads as zeroes.
    Sparse,
    /// Nothing says where its bytes are.
    Lost,
}

/// What [`copy_stored`] wrote and skipped, in bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StreamCounts {
    /// Stream offset of the first stored byte; for `$J` it equals that byte's USN.
    pub first_offset: u64,
    pub stored: u64,
    pub sparse: u64,
    pub lost: u64,
}

/// Bytes of `extents` that are not stored (sparse or lost). Only `$J` may have them: a gap in
/// `$MFT`, `$Boot` or `$SDS` would shift every later byte.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "used by the Windows reader")
)]
fn unstored(extents: &[Extent]) -> u64 {
    (extents.iter())
        .filter(|extent| extent.kind != ExtentKind::Stored)
        .fold(0, |sum, extent| sum.saturating_add(extent.length))
}

/// Copies the stored extents of `source` to `sink`, in order; sparse and lost extents are
/// counted, never padded (ADR 0005 #2). A stream without extents is copied whole.
pub fn copy_stored(
    source: &mut (impl Read + Seek),
    extents: &[Extent],
    sink: &mut impl Write,
) -> std::io::Result<StreamCounts> {
    let mut counts = StreamCounts::default();
    if extents.is_empty() {
        counts.stored = std::io::copy(source, sink)?;
        return Ok(counts);
    }
    let mut first = None;
    for extent in extents {
        match extent.kind {
            ExtentKind::Stored => {
                first.get_or_insert(extent.offset);
                source.seek(SeekFrom::Start(extent.offset))?;
                let copied = std::io::copy(&mut source.take(extent.length), sink)?;
                if copied < extent.length {
                    return Err(std::io::ErrorKind::UnexpectedEof.into());
                }
                counts.stored = counts.stored.saturating_add(copied);
            }
            ExtentKind::Sparse => counts.sparse = counts.sparse.saturating_add(extent.length),
            ExtentKind::Lost => counts.lost = counts.lost.saturating_add(extent.length),
        }
    }
    counts.first_offset = first.unwrap_or_default();
    Ok(counts)
}

/// A zip being written: one entry per [`Collection::add`], then `meta.json` (ADR 0024).
#[derive(Debug)]
pub struct Collection<W: Write> {
    archive: ZipArchiveWriter<W>,
    started: UtcDateTime,
}

impl<W: Write> Collection<W> {
    /// `started` is the collection start time, used as every entry's time.
    pub fn new(writer: W, started: Filetime) -> Self {
        // Seconds from 1601-01-01 to 1970-01-01.
        const UNIX_EPOCH: i64 = 11_644_473_600;
        let seconds = i64::try_from(started.raw() / 10_000_000).unwrap_or(i64::MAX);
        Self {
            archive: ZipArchiveWriter::new(writer),
            started: UtcDateTime::from_unix(seconds.saturating_sub(UNIX_EPOCH)),
        }
    }

    /// Adds entry `name`: the stored extents of `source` (see [`copy_stored`]), deflated.
    pub fn add(
        &mut self,
        name: &str,
        source: &mut (impl Read + Seek),
        extents: &[Extent],
    ) -> std::io::Result<StreamCounts> {
        let (mut entry, config) = self
            .archive
            .new_file(name)
            .compression_method(CompressionMethod::DEFLATE)
            .last_modified(self.started)
            .start()
            .map_err(std::io::Error::other)?;
        let mut writer = config.wrap(DeflateEncoder::new(&mut entry, Compression::default()));
        let counts = copy_stored(source, extents, &mut writer)?;
        let (encoder, descriptor) = writer.finish().map_err(std::io::Error::other)?;
        encoder.finish()?;
        entry.finish(descriptor).map_err(std::io::Error::other)?;
        Ok(counts)
    }

    /// Writes `meta.json` and the central directory; returns the writer.
    pub fn finish(mut self, meta: &Meta) -> std::io::Result<W> {
        let mut json = serde_json::to_vec_pretty(&MetaJson { format: 1, meta })?;
        json.push(b'\n');
        self.add("meta.json", &mut std::io::Cursor::new(json), &[])?;
        self.archive.finish().map_err(std::io::Error::other)
    }
}

/// `meta.json`: what the files cannot tell (ADR 0024).
#[derive(Debug, Serialize)]
pub struct Meta {
    /// `mozu` version and commit.
    pub tool: String,
    /// `COMPUTERNAME`; display only.
    pub host: String,
    pub volume: String,
    #[serde(serialize_with = "iso8601")]
    pub started: Filetime,
    #[serde(serialize_with = "iso8601")]
    pub finished: Filetime,
    pub files: Vec<FileSize>,
    /// `None` when the volume has no USN journal.
    pub j: Option<StreamCounts>,
}

/// An entry of the zip and the bytes written to it.
#[derive(Debug, Serialize)]
pub struct FileSize {
    pub name: String,
    pub size: u64,
}

/// `meta.json` as written: the format version first; a reader refuses another one.
#[derive(Serialize)]
struct MetaJson<'a> {
    format: u32,
    #[serde(flatten)]
    meta: &'a Meta,
}

fn iso8601<S: Serializer>(time: &Filetime, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&time.iso8601())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn extent(offset: u64, length: u64, kind: ExtentKind) -> Extent {
        Extent {
            offset,
            length,
            kind,
        }
    }

    fn meta() -> Meta {
        Meta {
            tool: "mozu 0.0.0 (abc1234)".to_owned(),
            host: "PC-042".to_owned(),
            volume: "C:".to_owned(),
            started: Filetime::from_raw(134_000_000_000_000_000),
            finished: Filetime::from_raw(134_000_000_100_000_000),
            files: Vec::new(),
            j: None,
        }
    }

    type Entry = (
        String,
        rawzip::CompressionMethod,
        rawzip::time::ZipDateTimeKind,
        Vec<u8>,
    );

    /// Entry names, compression methods, times and verified contents, in archive order.
    fn read_zip(zip: &[u8]) -> Result<Vec<Entry>, Box<dyn std::error::Error>> {
        let archive = rawzip::ZipArchive::from_slice(zip)?;
        let mut entries = archive.entries();
        let mut read = Vec::new();
        while let Some(entry) = entries.next_entry()? {
            let name = String::from_utf8(entry.file_path().as_ref().to_vec())?;
            let local = archive.get_entry(entry.wayfinder())?;
            let decoder = flate2::bufread::DeflateDecoder::new(local.data());
            let mut content = Vec::new();
            local.verifying_reader(decoder).read_to_end(&mut content)?;
            read.push((
                name,
                entry.compression_method(),
                entry.last_modified(),
                content,
            ));
        }
        Ok(read)
    }

    #[test]
    fn meta_json_is_the_format_version_then_the_fields_in_adr_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let meta = Meta {
            files: vec![
                FileSize {
                    name: "$MFT".to_owned(),
                    size: 5,
                },
                FileSize {
                    name: "$J".to_owned(),
                    size: 3,
                },
            ],
            j: Some(StreamCounts {
                first_offset: 4,
                stored: 3,
                sparse: 4,
                lost: 1,
            }),
            ..meta()
        };
        let zip = Collection::new(Vec::new(), meta.started).finish(&meta)?;
        let entries = read_zip(&zip)?;
        let json = entries.last().map(|(.., content)| content.as_slice());
        let expected = r#"{
  "format": 1,
  "tool": "mozu 0.0.0 (abc1234)",
  "host": "PC-042",
  "volume": "C:",
  "started": "2025-08-18T14:13:20.0000000Z",
  "finished": "2025-08-18T14:13:30.0000000Z",
  "files": [
    {
      "name": "$MFT",
      "size": 5
    },
    {
      "name": "$J",
      "size": 3
    }
  ],
  "j": {
    "first_offset": 4,
    "stored": 3,
    "sparse": 4,
    "lost": 1
  }
}
"#;
        assert_eq!(json, Some(expected.as_bytes()));
        Ok(())
    }

    #[test]
    fn the_same_inputs_and_start_time_give_a_byte_identical_zip() -> std::io::Result<()> {
        let zip = || {
            let mut collection = Collection::new(Vec::new(), meta().started);
            collection.add("$MFT", &mut Cursor::new(b"FILE0".repeat(1000)), &[])?;
            collection.finish(&meta())
        };
        assert_eq!(zip()?, zip()?);
        Ok(())
    }

    #[test]
    fn without_a_journal_there_is_no_j_entry_and_j_is_null()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut collection = Collection::new(Vec::new(), meta().started);
        for name in ["$MFT", "$Boot", "$SDS"] {
            collection.add(name, &mut Cursor::new(b"x".to_vec()), &[])?;
        }
        let zip = collection.finish(&meta())?;
        let entries = read_zip(&zip)?;
        let names: Vec<_> = entries.iter().map(|(name, ..)| name.as_str()).collect();
        assert_eq!(names, ["$MFT", "$Boot", "$SDS", "meta.json"]);
        let json = entries
            .last()
            .map(|(.., content)| String::from_utf8_lossy(content));
        assert!(json.is_some_and(|json| json.ends_with("\n  \"j\": null\n}\n")));
        Ok(())
    }

    #[test]
    fn entries_are_deflated_in_the_order_added_and_meta_json_comes_last()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut collection = Collection::new(Vec::new(), meta().started);
        collection.add("$MFT", &mut Cursor::new(b"FILE0".to_vec()), &[])?;
        let journal = [
            extent(0, 4, ExtentKind::Sparse),
            extent(4, 3, ExtentKind::Stored),
        ];
        collection.add("$J", &mut Cursor::new(b"\0\0\0\0usn".to_vec()), &journal)?;
        collection.add("$Boot", &mut Cursor::new(b"NTFS".to_vec()), &[])?;
        collection.add("$SDS", &mut Cursor::new(b"sds".to_vec()), &[])?;
        let zip = collection.finish(&meta())?;

        let entries = read_zip(&zip)?;
        let names: Vec<_> = entries.iter().map(|(name, ..)| name.as_str()).collect();
        assert_eq!(names, ["$MFT", "$J", "$Boot", "$SDS", "meta.json"]);
        assert!(
            entries
                .iter()
                .all(|(_, method, ..)| *method == rawzip::CompressionMethod::DEFLATE)
        );
        // 134_000_000_000_000_000 = 2025-08-18 14:13:20 UTC.
        let started =
            rawzip::time::ZipDateTimeKind::Utc(rawzip::time::UtcDateTime::from_unix(1_755_526_400));
        assert!(entries.iter().all(|(.., time, _)| *time == started));
        let contents: Vec<_> = entries
            .iter()
            .take(4)
            .map(|(.., content)| content.as_slice())
            .collect();
        assert_eq!(contents, [&b"FILE0"[..], b"usn", b"NTFS", b"sds"]);
        Ok(())
    }

    #[test]
    fn only_stored_extents_are_written_and_the_rest_is_counted() -> std::io::Result<()> {
        let mut source = Cursor::new(b"aaSSbbLLc".to_vec());
        let extents = [
            extent(0, 2, ExtentKind::Stored),
            extent(2, 2, ExtentKind::Sparse),
            extent(4, 2, ExtentKind::Stored),
            extent(6, 2, ExtentKind::Lost),
            extent(8, 1, ExtentKind::Stored),
        ];
        let mut sink = Vec::new();
        let counts = copy_stored(&mut source, &extents, &mut sink)?;
        assert_eq!(sink, b"aabbc");
        assert_eq!(
            counts,
            StreamCounts {
                first_offset: 0,
                stored: 5,
                sparse: 2,
                lost: 2
            }
        );
        Ok(())
    }

    #[test]
    fn first_offset_is_where_the_first_stored_byte_sits_after_a_sparse_region()
    -> std::io::Result<()> {
        let mut source = Cursor::new(b"\0\0\0\0usn".to_vec());
        let extents = [
            extent(0, 4, ExtentKind::Sparse),
            extent(4, 3, ExtentKind::Stored),
        ];
        let mut sink = Vec::new();
        let counts = copy_stored(&mut source, &extents, &mut sink)?;
        assert_eq!(sink, b"usn");
        assert_eq!(counts.first_offset, 4);
        Ok(())
    }

    #[test]
    fn a_stream_without_extents_is_copied_whole() -> std::io::Result<()> {
        let mut source = Cursor::new(b"resident".to_vec());
        let mut sink = Vec::new();
        let counts = copy_stored(&mut source, &[], &mut sink)?;
        assert_eq!(sink, b"resident");
        assert_eq!(
            counts,
            StreamCounts {
                first_offset: 0,
                stored: 8,
                sparse: 0,
                lost: 0
            }
        );
        Ok(())
    }

    #[test]
    fn unstored_is_the_sparse_and_lost_bytes() {
        let extents = [
            extent(0, 2, ExtentKind::Stored),
            extent(2, 3, ExtentKind::Sparse),
            extent(5, 4, ExtentKind::Lost),
        ];
        assert_eq!(unstored(&extents), 7);
        assert_eq!(unstored(&extents[..1]), 0);
    }

    #[test]
    fn a_source_shorter_than_its_extents_is_an_error() {
        let mut source = Cursor::new(b"abc".to_vec());
        let extents = [extent(0, 5, ExtentKind::Stored)];
        let error = copy_stored(&mut source, &extents, &mut Vec::new()).err();
        assert_eq!(
            error.map(|e| e.kind()),
            Some(std::io::ErrorKind::UnexpectedEof)
        );
    }
}
