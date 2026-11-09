//! Save chat with file: the desktop app's assistant conversation about a drawing, written into its
//! DXF file when the person turns it on.
//!
//! The conversation (JSON, as base64) is a run of DXF comment groups (999) at the top of the file,
//! each value starting with [`PREFIX`]: CAD programs skip comments, and so does CADCraft's reader.
//! Binary DXF and DWG files are saved as they are. The desktop app sets a [`ChatHook`]; without
//! one (automation, the web app) a drawing is written as it is. A save writes the file afresh, so
//! a drawing saved with the option off has no conversation in it.

use std::sync::OnceLock;

use serde_json::Value;

/// What each comment that holds part of the conversation starts with.
pub const PREFIX: &str = "ARTCRAFT-ASSISTANT:";
/// Base64 characters per comment: short enough for any reader's string limit.
const CHUNK: usize = 200;
/// The most comment lines read back (8 MiB of base64).
const MAX_LINES: usize = (8 << 20) / CHUNK;

/// The conversation to save in the drawing file at a path (None: none, or the option off).
pub type ChatHook = Box<dyn Fn(&str) -> Option<Value> + Send + Sync>;

static HOOK: OnceLock<ChatHook> = OnceLock::new();

/// Sets the [`ChatHook`] (once per run).
pub fn set_hook(hook: ChatHook) {
    let _ = HOOK.set(hook);
}

/// The drawing file `bytes` about to be written to `path` (a DXF file), with the conversation the
/// hook gives for it.
pub(crate) fn hooked(path: &str, bytes: Vec<u8>) -> Vec<u8> {
    match HOOK.get().and_then(|hook| hook(path)) {
        Some(chat) => with_chat(bytes, &chat),
        None => bytes,
    }
}

/// Whether `bytes` are a DXF file in text form (binary DXF starts with its sentinel).
fn is_text_dxf(bytes: &[u8]) -> bool {
    !bytes.starts_with(b"AutoCAD Binary DXF") && bytes.iter().take(64).all(|b| b.is_ascii())
}

/// The DXF text `bytes` with `chat` in comments at the top. Anything else is returned as it is.
pub fn with_chat(bytes: Vec<u8>, chat: &Value) -> Vec<u8> {
    if !is_text_dxf(&bytes) {
        return bytes;
    }
    let nl = if bytes.windows(2).take(64).any(|w| w == b"\r\n") { "\r\n" } else { "\n" };
    let text = base64(chat.to_string().as_bytes());
    let mut out = String::with_capacity(text.len() + text.len() / CHUNK * 32 + 32);
    for chunk in text.as_bytes().chunks(CHUNK) {
        out.push_str("999");
        out.push_str(nl);
        out.push_str(PREFIX);
        out.push_str(&String::from_utf8_lossy(chunk));
        out.push_str(nl);
    }
    let mut out = out.into_bytes();
    out.extend_from_slice(&bytes);
    out
}

/// The conversation in the comments at the top of a DXF file, if any.
pub fn chat_in(bytes: &[u8]) -> Option<Value> {
    if !is_text_dxf(bytes) {
        return None;
    }
    let mut text = String::new();
    let mut lines = bytes.split(|b| *b == b'\n').map(|l| String::from_utf8_lossy(l).trim().to_string());
    for _ in 0..MAX_LINES {
        let Some(code) = lines.next() else { break };
        if code != "999" {
            break;
        }
        let value = lines.next()?;
        match value.strip_prefix(PREFIX) {
            Some(part) => text.push_str(part),
            None if text.is_empty() => continue,
            None => break,
        }
    }
    serde_json::from_slice(&unbase64(&text)?).ok()
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = c.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= c.len() {
                out.push(char::from(ALPHABET.get(((n >> (18 - 6 * i)) & 63) as usize).copied().unwrap_or(b'A')));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn unbase64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut n, mut bits) = (0u32, 0u32);
    for c in text.bytes().filter(|c| *c != b'=') {
        let v = ALPHABET.iter().position(|a| *a == c)? as u32;
        n = (n << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
            n &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_chat_rides_in_comments_and_the_drawing_still_opens() {
        let chat = json!({"version": 1, "messages": [{"role": "user", "text": "Vẽ mặt bằng tầng 1 ".repeat(40)}]});
        let dxf = b"  0\r\nSECTION\r\n  2\r\nENTITIES\r\n  0\r\nENDSEC\r\n  0\r\nEOF\r\n".to_vec();
        let with = with_chat(dxf.clone(), &chat);
        assert!(with.ends_with(&dxf), "the drawing itself is as it was");
        assert!(with.starts_with(b"999\r\nARTCRAFT-ASSISTANT:"), "comments in the file's own line endings");
        assert_eq!(chat_in(&with), Some(chat.clone()));
        assert_eq!(chat_in(&dxf), None);
        let binary = b"AutoCAD Binary DXF\r\n\x1a\0".to_vec();
        assert_eq!(with_chat(binary.clone(), &chat), binary, "binary DXF: as it is");
        for s in ["", "a", "ab", "abc", "Tiếng Việt"] {
            assert_eq!(unbase64(&base64(s.as_bytes())).unwrap(), s.as_bytes());
        }
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
    }
}
