//! Encodings used by the game's data: URL-safe base64 wrapped around gzip or zlib.

use std::io::Read;

use base64::Engine;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use flate2::read::{GzDecoder, ZlibDecoder};

#[derive(Debug)]
pub enum DecodeError {
    Base64(base64::DecodeError),
    Inflate(std::io::Error),
    Utf8(std::string::FromUtf8Error),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Base64(e) => write!(f, "base64: {e}"),
            DecodeError::Inflate(e) => write!(f, "inflate: {e}"),
            DecodeError::Utf8(e) => write!(f, "utf-8: {e}"),
        }
    }
}

impl std::error::Error for DecodeError {}

const LENIENT: GeneralPurposeConfig = GeneralPurposeConfig::new()
    .with_decode_padding_mode(DecodePaddingMode::Indifferent)
    .with_decode_allow_trailing_bits(true);
const URL_SAFE: GeneralPurpose = GeneralPurpose::new(&base64::alphabet::URL_SAFE, LENIENT);
const STANDARD: GeneralPurpose = GeneralPurpose::new(&base64::alphabet::STANDARD, LENIENT);

/// Decode base64 in either alphabet, padded or not, ignoring surrounding whitespace.
pub fn base64_decode(text: &str) -> Result<Vec<u8>, DecodeError> {
    let text = text.trim().trim_end_matches('\0');
    URL_SAFE
        .decode(text)
        .or_else(|_| STANDARD.decode(text))
        .map_err(DecodeError::Base64)
}

/// Encode in the URL-safe alphabet with padding (what the game sends in `chk` fields).
pub fn base64_encode_url(data: &[u8]) -> String {
    use base64::Engine;
    URL_SAFE.encode(data)
}

/// Inflate gzip (`1f 8b`) or zlib data; anything else is returned unchanged.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let mut out = Vec::with_capacity(data.len() * 8);
    match data {
        [0x1f, 0x8b, ..] => GzDecoder::new(data).read_to_end(&mut out),
        [0x78, b, ..] if (u16::from(0x78u8) << 8 | u16::from(*b)) % 31 == 0 => {
            ZlibDecoder::new(data).read_to_end(&mut out)
        }
        _ => return Ok(data.to_vec()),
    }
    .map_err(DecodeError::Inflate)?;
    Ok(out)
}

/// Decode a level file's raw bytes. The game reads these as C strings, so anything after the
/// first NUL is ignored (two official Tower levels end in `\0` plus a stray byte).
pub fn decode_level_bytes(bytes: &[u8]) -> Result<String, DecodeError> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    decode_level_string(&String::from_utf8_lossy(&bytes[..end]))
}

/// Decode a level's object string as stored in level files and save data.
/// Already-decoded strings (they start with a `k` settings key) pass through unchanged.
pub fn decode_level_string(text: &str) -> Result<String, DecodeError> {
    let text = text.trim();
    if text.starts_with('k') {
        return Ok(text.to_owned());
    }
    let raw = inflate(&base64_decode(text)?)?;
    String::from_utf8(raw).map_err(DecodeError::Utf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_level_strings_pass_through() {
        assert_eq!(decode_level_string("kS38,1;1,1,2,15,3,15;").unwrap(), "kS38,1;1,1,2,15,3,15;");
    }

    #[test]
    fn gzip_round_trip() {
        use flate2::{Compression, write::GzEncoder};
        use std::io::Write;
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(b"kA13,0;1,8,2,45,3,15;").unwrap();
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(gz.finish().unwrap());
        assert_eq!(decode_level_string(&encoded).unwrap(), "kA13,0;1,8,2,45,3,15;");
    }
}
