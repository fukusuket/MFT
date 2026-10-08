//! Standard Base64 (RFC 4648 §4) with padding, for embedding report data.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
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
        for (i, sextet) in sextets.into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[usize::from(sextet)]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
