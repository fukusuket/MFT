//! Standard Base64 (RFC 4648 §4) with padding, for embedding report data.

use std::io::Write;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Four output characters for one to three input bytes, padded with `=`.
fn quad(chunk: &[u8]) -> [u8; 4] {
    let b = [
        chunk[0],
        *chunk.get(1).unwrap_or(&0),
        *chunk.get(2).unwrap_or(&0),
    ];
    let sextets = [
        b[0] >> 2,
        (b[0] & 0x03) << 4 | b[1] >> 4,
        (b[1] & 0x0f) << 2 | b[2] >> 6,
        b[2] & 0x3f,
    ];
    // A chunk of n bytes carries n + 1 sextets; the rest is padding.
    let mut out = [b'='; 4];
    for (i, sextet) in sextets.into_iter().enumerate().take(chunk.len() + 1) {
        out[i] = ALPHABET[usize::from(sextet)];
    }
    out
}

/// Streams Base64 of everything written to it into `out`; [`Encoder::finish`] adds the padding.
#[derive(Debug)]
pub(crate) struct Encoder<W: Write> {
    out: W,
    /// Input bytes not yet encoded: fewer than three.
    pending: Vec<u8>,
}

impl<W: Write> Encoder<W> {
    pub(crate) fn new(out: W) -> Self {
        Self {
            out,
            pending: Vec::with_capacity(3),
        }
    }

    pub(crate) fn finish(mut self) -> std::io::Result<W> {
        if !self.pending.is_empty() {
            self.out.write_all(&quad(&self.pending))?;
        }
        Ok(self.out)
    }
}

impl<W: Write> Write for Encoder<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut input = bytes;
        if !self.pending.is_empty() {
            let take = (3 - self.pending.len()).min(input.len());
            self.pending.extend_from_slice(&input[..take]);
            input = &input[take..];
            if self.pending.len() < 3 {
                return Ok(bytes.len());
            }
            self.out.write_all(&quad(&self.pending))?;
            self.pending.clear();
        }
        let whole = input.len() / 3 * 3;
        let encoded: Vec<u8> = input[..whole].chunks(3).flat_map(quad).collect();
        self.out.write_all(&encoded)?;
        self.pending.extend_from_slice(&input[whole..]);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(bytes: &[u8]) -> String {
        let mut encoder = Encoder::new(Vec::new());
        let encoded = encoder
            .write_all(bytes)
            .and_then(|()| encoder.finish())
            .unwrap_or_default();
        String::from_utf8(encoded).unwrap_or_default()
    }

    #[test]
    fn split_writes_match_a_single_write() -> std::io::Result<()> {
        let input = b"any carnal pleasure.";
        for len in 0..=input.len() {
            for split in 0..=len {
                let (a, b) = input[..len].split_at(split);
                let mut encoder = Encoder::new(Vec::new());
                encoder.write_all(a)?;
                encoder.write_all(b)?;
                let streamed = encoder.finish()?;
                assert_eq!(
                    streamed,
                    encode(&input[..len]).into_bytes(),
                    "len {len} split {split}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn matches_rfc_4648_vectors() {
        let vectors = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (input, expected) in vectors {
            assert_eq!(encode(input.as_bytes()), expected, "input {input:?}");
        }
    }

    #[test]
    fn uses_the_upper_alphabet() {
        assert_eq!(encode(&[0xfb, 0xff, 0xbf]), "+/+/");
    }
}
