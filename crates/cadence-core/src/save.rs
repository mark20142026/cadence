//! Save files (`CCGameManager.dat`, `CCLocalLevels.dat`) and the game's compact plist dialect.
//!
//! macOS: AES-256-ECB with PKCS#7 padding around the XML (no IV). The key is the game's
//! fixed save key, verified by decrypting real saves.
//! Windows: XOR every byte with 11, then URL-safe base64, then gzip.

use aes::Aes256;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit, generic_array::GenericArray};

use crate::codec::{self, DecodeError};

const MAC_KEY: &[u8; 32] = b"ipu9TUv54yv]isFMh5@;t.5w34E2Ry@{";
const WINDOWS_XOR: u8 = 11;

#[derive(Debug)]
pub enum SaveError {
    Decode(DecodeError),
    BadLength,
    BadPadding,
    Utf8,
    Xml(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Decode(e) => write!(f, "{e}"),
            SaveError::BadLength => f.write_str("encrypted save is not a whole number of AES blocks"),
            SaveError::BadPadding => f.write_str("bad PKCS#7 padding (wrong key or not a macOS save)"),
            SaveError::Utf8 => f.write_str("save is not UTF-8"),
            SaveError::Xml(e) => write!(f, "plist: {e}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// Decrypt a macOS save to its XML text.
pub fn decrypt_mac(data: &[u8]) -> Result<String, SaveError> {
    if data.is_empty() || data.len() % 16 != 0 {
        return Err(SaveError::BadLength);
    }
    let cipher = Aes256::new(GenericArray::from_slice(MAC_KEY));
    let mut out = data.to_vec();
    for block in out.chunks_exact_mut(16) {
        cipher.decrypt_block(GenericArray::from_mut_slice(block));
    }
    let pad = *out.last().unwrap() as usize;
    if pad == 0 || pad > 16 || out[out.len() - pad..].iter().any(|&b| b as usize != pad) {
        return Err(SaveError::BadPadding);
    }
    out.truncate(out.len() - pad);
    String::from_utf8(out).map_err(|_| SaveError::Utf8)
}

/// Encrypt XML text as a macOS save.
pub fn encrypt_mac(xml: &str) -> Vec<u8> {
    let cipher = Aes256::new(GenericArray::from_slice(MAC_KEY));
    let mut out = xml.as_bytes().to_vec();
    let pad = 16 - out.len() % 16;
    out.extend(std::iter::repeat_n(pad as u8, pad));
    for block in out.chunks_exact_mut(16) {
        cipher.encrypt_block(GenericArray::from_mut_slice(block));
    }
    out
}

/// Decode a Windows (or Android) save to its XML text.
pub fn decode_windows(data: &[u8]) -> Result<String, SaveError> {
    let text: Vec<u8> = data.iter().map(|b| b ^ WINDOWS_XOR).collect();
    let text = String::from_utf8_lossy(&text);
    let raw = codec::inflate(&codec::base64_decode(&text).map_err(SaveError::Decode)?)
        .map_err(SaveError::Decode)?;
    String::from_utf8(raw).map_err(|_| SaveError::Utf8)
}

/// Decode a save from either platform, detected from the content.
pub fn decode_any(data: &[u8]) -> Result<String, SaveError> {
    if data.starts_with(b"<?xml") {
        return String::from_utf8(data.to_vec()).map_err(|_| SaveError::Utf8);
    }
    decrypt_mac(data).or_else(|_| decode_windows(data))
}

/// A value in the game's plist dialect. Numbers keep their exact text for lossless round trips.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Dict(Dict),
    Str(String),
    Int(String),
    Real(String),
    True,
}

pub type Dict = Vec<(String, Value)>;

impl Value {
    pub fn as_dict(&self) -> Option<&Dict> {
        match self {
            Value::Dict(d) => Some(d),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) | Value::Int(s) | Value::Real(s) => Some(s),
            _ => None,
        }
    }
}

pub fn get<'a>(dict: &'a Dict, key: &str) -> Option<&'a Value> {
    dict.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Parse the root `<dict>` of a save's XML.
pub fn parse_plist(xml: &str) -> Result<Dict, SaveError> {
    let start = xml.find("<dict>").ok_or_else(|| SaveError::Xml("no <dict>".into()))?;
    let mut p = Parser { s: &xml[start + "<dict>".len()..] };
    p.dict_body("</dict>")
}

/// Write a root dict back as save XML.
pub fn write_plist(dict: &Dict) -> String {
    let mut out = String::from(r#"<?xml version="1.0"?><plist version="1.0" gjver="2.0"><dict>"#);
    write_dict_body(&mut out, dict);
    out.push_str("</dict></plist>");
    out
}

struct Parser<'a> {
    s: &'a str,
}

impl Parser<'_> {
    fn eat(&mut self, token: &str) -> bool {
        if let Some(rest) = self.s.strip_prefix(token) {
            self.s = rest;
            true
        } else {
            false
        }
    }

    fn text_until(&mut self, close: &str) -> Result<String, SaveError> {
        let end = self.s.find(close).ok_or_else(|| SaveError::Xml(format!("missing {close}")))?;
        let text = unescape(&self.s[..end]);
        self.s = &self.s[end + close.len()..];
        Ok(text)
    }

    fn dict_body(&mut self, close: &str) -> Result<Dict, SaveError> {
        let mut dict = Vec::new();
        loop {
            if self.eat(close) {
                return Ok(dict);
            }
            if !self.eat("<k>") {
                let near: String = self.s.chars().take(40).collect();
                return Err(SaveError::Xml(format!("expected <k> near {near:?}")));
            }
            let key = self.text_until("</k>")?;
            let value = self.value()?;
            dict.push((key, value));
        }
    }

    fn value(&mut self) -> Result<Value, SaveError> {
        if self.eat("<d>") {
            Ok(Value::Dict(self.dict_body("</d>")?))
        } else if self.eat("<d />") || self.eat("<d/>") {
            Ok(Value::Dict(Vec::new()))
        } else if self.eat("<s>") {
            Ok(Value::Str(self.text_until("</s>")?))
        } else if self.eat("<s />") || self.eat("<s/>") {
            Ok(Value::Str(String::new()))
        } else if self.eat("<i>") {
            Ok(Value::Int(self.text_until("</i>")?))
        } else if self.eat("<r>") {
            Ok(Value::Real(self.text_until("</r>")?))
        } else if self.eat("<t />") || self.eat("<t/>") {
            Ok(Value::True)
        } else {
            let near: String = self.s.chars().take(40).collect();
            Err(SaveError::Xml(format!("unknown value near {near:?}")))
        }
    }
}

fn write_dict_body(out: &mut String, dict: &Dict) {
    for (key, value) in dict {
        out.push_str("<k>");
        escape_into(out, key);
        out.push_str("</k>");
        match value {
            Value::Dict(d) if d.is_empty() => out.push_str("<d />"),
            Value::Dict(d) => {
                out.push_str("<d>");
                write_dict_body(out, d);
                out.push_str("</d>");
            }
            Value::Str(s) => {
                out.push_str("<s>");
                escape_into(out, s);
                out.push_str("</s>");
            }
            Value::Int(s) => {
                out.push_str("<i>");
                out.push_str(s);
                out.push_str("</i>");
            }
            Value::Real(s) => {
                out.push_str("<r>");
                out.push_str(s);
                out.push_str("</r>");
            }
            Value::True => out.push_str("<t />"),
        }
    }
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn escape_into(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_round_trip() {
        let xml = r#"<?xml version="1.0"?><plist version="1.0" gjver="2.0"><dict><k>a</k><i>1</i></dict></plist>"#;
        assert_eq!(decrypt_mac(&encrypt_mac(xml)).unwrap(), xml);
    }

    #[test]
    fn plist_round_trip() {
        let xml = r#"<?xml version="1.0"?><plist version="1.0" gjver="2.0"><dict><k>LLM_01</k><d><k>_isArr</k><t /></d><k>LLM_02</k><i>51</i><k>n</k><s>a &amp; b</s><k>e</k><d /><k>r</k><r>0.5</r></dict></plist>"#;
        let dict = parse_plist(xml).unwrap();
        assert_eq!(get(&dict, "LLM_02").and_then(Value::as_str), Some("51"));
        assert_eq!(get(&dict, "n").and_then(Value::as_str), Some("a & b"));
        assert_eq!(write_plist(&dict), xml);
    }
}
