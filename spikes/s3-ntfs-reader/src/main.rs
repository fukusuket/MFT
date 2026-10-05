//! Phase 0 spike S3: can `ntfs-reader` collect `$MFT`, `$J` (without the sparse region),
//! `$Secure:$SDS`, `$Boot` (and `$UpCase`) from a live volume, with no `unsafe` in our code?
//! Usage (elevated): `s3-ntfs-reader.exe \\.\C: <out-dir>`. See README.md.

#![forbid(unsafe_code)]

#[cfg(not(windows))]
fn main() {
    eprintln!("s3-ntfs-reader runs on Windows only");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    if let Err(e) = windows_main::run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod windows_main {
    use std::error::Error;
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::path::Path;
    use std::time::Instant;

    use ntfs_reader::{ExtentLocation, Mft, NtfsFile, Volume};

    type Res<T> = Result<T, Box<dyn Error>>;

    const MFT: u64 = 0;
    const BOOT: u64 = 7;
    const SECURE: u64 = 9;
    const UPCASE: u64 = 10;
    const EXTEND: u64 = 11;

    pub(crate) fn run() -> Res<()> {
        let mut args = std::env::args().skip(1);
        let volume_path = args
            .next()
            .ok_or("usage: s3-ntfs-reader <\\\\.\\C:> <out-dir>")?;
        let out = args.next().ok_or("missing <out-dir>")?;
        let out = Path::new(&out);
        std::fs::create_dir_all(out)?;

        let t = Instant::now();
        let volume = Volume::new(&volume_path)?;
        println!(
            "[volume] {volume_path}: cluster {} B, record {} B, size {} B",
            volume.cluster_size(),
            volume.file_record_size(),
            volume.volume_size()
        );
        let mft = Mft::new(volume)?;
        println!(
            "[mft] records {}, corrupt {}, loaded in {:.2?}",
            mft.record_count(),
            mft.corrupt_records(),
            t.elapsed()
        );

        let record = |n: u64| mft.record(n).ok_or_else(|| format!("record {n} not found"));
        copy_stream(&record(MFT)?, None, &out.join("MFT"))?;
        copy_stream(&record(BOOT)?, None, &out.join("Boot"))?;
        copy_stream(&record(SECURE)?, Some("$SDS"), &out.join("SDS"))?;
        copy_stream(&record(UPCASE)?, None, &out.join("UpCase"))?;

        let usn = mft
            .files()
            .find(|f| {
                f.names().any(|n| {
                    n.parent_number() == EXTEND && n.to_os_string() == OsStr::new("$UsnJrnl")
                })
            })
            .ok_or("$Extend\\$UsnJrnl not found")?;
        copy_stream(&usn, Some("$J"), &out.join("J"))?;

        println!("[done] {:.2?}", t.elapsed());
        Ok(())
    }

    /// Copy only the stored parts of a stream; sparse parts are skipped and reported.
    fn copy_stream(file: &NtfsFile<'_>, stream: Option<&str>, dest: &Path) -> Res<()> {
        let t = Instant::now();
        let mut reader = file.open_stream(stream.map(OsStr::new))?;
        let mut sink = File::create(dest)?;
        let (mut stored, mut sparse, mut lost) = (0u64, 0u64, 0u64);
        let extents = reader.extents().to_vec();
        if extents.is_empty() {
            stored += std::io::copy(&mut reader, &mut sink)?;
        }
        for e in extents {
            match e.location {
                ExtentLocation::Sparse => sparse += e.length,
                ExtentLocation::Resident | ExtentLocation::Volume { .. } => {
                    reader.seek(SeekFrom::Start(e.stream_offset))?;
                    stored += std::io::copy(&mut (&mut reader).take(e.length), &mut sink)?;
                }
                _ => lost += e.length,
            }
        }
        sink.flush()?;
        println!(
            "[{}] stream {:?}: logical {} B, written {} B, sparse skipped {} B, lost {} B, {:.2?}",
            dest.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
            stream.unwrap_or("(default)"),
            reader.size(),
            stored,
            sparse,
            lost,
            t.elapsed()
        );
        Ok(())
    }
}
