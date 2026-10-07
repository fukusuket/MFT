//! Windows baseline: which files a clean install has. Keys are `NormPath` segments with the
//! ADR 0012 rules applied; the same function builds and looks up keys (ADR 0002 #2).

mod normalize;
mod vwr;

use std::io::{Read, Write};

use normalize::NORMALIZATION_VERSION;

/// Fatal problems building or loading a baseline.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading VanillaWindowsReference CSV: {0}")]
    Csv(#[from] csv::Error),
    #[error("baseline index: {0}")]
    Fst(#[from] fst::Error),
    #[error("writing baseline: {0}")]
    Io(#[from] std::io::Error),
    #[error("VanillaWindowsReference CSV has no FullName column")]
    NoFullNameColumn,
    #[error("not a baseline file")]
    NotABaseline,
    #[error("baseline uses normalization version {0}; this build expects {NORMALIZATION_VERSION}")]
    NormalizationVersion(u32),
}

/// File layout: magic, normalization version (`u32` LE), then the fst set.
const MAGIC: [u8; 8] = *b"NTFSBL\0\0";
const HEADER_LEN: usize = MAGIC.len() + 4;

/// UTF-16 units as big-endian bytes, so byte order equals unit order (fst needs sorted keys).
fn big_endian(key: &[u16]) -> Vec<u8> {
    key.iter().flat_map(|u| u.to_be_bytes()).collect()
}

/// Whether a file is part of the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Standard,
    Outside,
}

/// Builds a baseline file from a VanillaWindowsReference CSV.
pub fn build(vwr: impl Read, mut out: impl Write) -> Result<(), Error> {
    let keys = vwr::keys(vwr)?;
    out.write_all(&MAGIC)?;
    out.write_all(&NORMALIZATION_VERSION.to_le_bytes())?;
    let mut set = fst::SetBuilder::new(out)?;
    for key in &keys {
        set.insert(big_endian(key))?;
    }
    set.finish()?;
    Ok(())
}

/// A loaded baseline.
#[derive(Debug)]
pub struct Baseline {
    keys: fst::Set<Vec<u8>>,
}

impl Baseline {
    pub fn load(mut file: Vec<u8>) -> Result<Self, Error> {
        let Some((magic, version)) = file.split_first_chunk::<8>().and_then(|(magic, rest)| {
            rest.first_chunk::<4>()
                .map(|v| (*magic, u32::from_le_bytes(*v)))
        }) else {
            return Err(Error::NotABaseline);
        };
        if magic != MAGIC {
            return Err(Error::NotABaseline);
        }
        if version != NORMALIZATION_VERSION {
            return Err(Error::NormalizationVersion(version));
        }
        file.drain(..HEADER_LEN);
        let keys = fst::Set::new(file)?;
        // fst trusts its input; the checksum catches corrupted files (not crafted ones).
        keys.as_fst().verify()?;
        Ok(Self { keys })
    }

    /// Status of a file given its path segments from the root down.
    pub fn status(&self, segments: &[&[u16]]) -> Status {
        match normalize::key(segments) {
            Some(key) if self.keys.contains(big_endian(&key)) => Status::Standard,
            _ => Status::Outside,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VWR: &str = r#""DirectoryName","Name","FullName","Length"
"C:\Windows","notepad.exe","C:\Windows\notepad.exe","1"
"C:\Users\alice","NTUSER.DAT","C:\Users\alice\NTUSER.DAT","1"
"#;

    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn status(baseline: &Baseline, path: &[&str]) -> Status {
        let segments: Vec<Vec<u16>> = path.iter().map(|s| units(s)).collect();
        let refs: Vec<&[u16]> = segments.iter().map(Vec::as_slice).collect();
        baseline.status(&refs)
    }

    #[test]
    fn built_file_loads_and_looks_up_standard_and_outside() -> Result<(), Error> {
        let mut file = Vec::new();
        build(VWR.as_bytes(), &mut file)?;

        let baseline = Baseline::load(file)?;

        assert_eq!(
            status(&baseline, &["WINDOWS", "Notepad.EXE"]),
            Status::Standard
        );
        assert_eq!(
            status(&baseline, &["Users", "bob", "ntuser.dat"]),
            Status::Standard
        );
        assert_eq!(status(&baseline, &["Windows", "calc.exe"]), Status::Outside);
        assert_eq!(
            status(&baseline, &["Windows\\notepad.exe"]),
            Status::Outside
        );
        Ok(())
    }

    #[test]
    fn wrong_magic_or_normalization_version_is_an_error() -> Result<(), Error> {
        let mut file = Vec::new();
        build(VWR.as_bytes(), &mut file)?;

        let mut wrong_magic = file.clone();
        wrong_magic[0] ^= 0xFF;
        assert!(matches!(
            Baseline::load(wrong_magic),
            Err(Error::NotABaseline)
        ));

        let mut other_version = file.clone();
        other_version[8..12].copy_from_slice(&(NORMALIZATION_VERSION + 1).to_le_bytes());
        assert!(matches!(
            Baseline::load(other_version),
            Err(Error::NormalizationVersion(v)) if v == NORMALIZATION_VERSION + 1
        ));

        assert!(matches!(
            Baseline::load(b"NTFS".to_vec()),
            Err(Error::NotABaseline)
        ));
        Ok(())
    }

    #[test]
    fn corrupted_index_is_an_error() -> Result<(), Error> {
        let mut file = Vec::new();
        build(VWR.as_bytes(), &mut file)?;
        let middle = HEADER_LEN + (file.len() - HEADER_LEN) / 2;
        file[middle] ^= 0x55;

        assert!(Baseline::load(file).is_err());
        Ok(())
    }
}
