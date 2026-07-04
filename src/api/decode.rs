use std::io::Read;

use flate2::read::{GzDecoder, ZlibDecoder};

/// View-time decode chain, run lazily only for `?decode=1`: reverse
/// `content-encoding`, then pretty-print if the payload is JSON. Capture stays
/// fast and honest — raw bytes are what's stored; this is a rendering.
pub fn decode_body(raw: &[u8], content_encoding: Option<&str>, content_type: Option<&str>) -> Vec<u8> {
    let decompressed = match content_encoding.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("gzip") | Some("x-gzip") => decompress(GzDecoder::new(raw), raw),
        Some("deflate") => decompress(ZlibDecoder::new(raw), raw),
        _ => raw.to_vec(),
    };

    if is_json(content_type) {
        pretty_json(decompressed)
    } else {
        decompressed
    }
}

fn decompress<R: Read>(mut decoder: R, raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    match decoder.read_to_end(&mut out) {
        Ok(_) => out,
        Err(_) => raw.to_vec(),
    }
}

fn is_json(content_type: Option<&str>) -> bool {
    content_type
        .map(|ct| ct.to_ascii_lowercase().contains("json"))
        .unwrap_or(false)
}

fn pretty_json(bytes: Vec<u8>) -> Vec<u8> {
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(value) => serde_json::to_vec_pretty(&value).unwrap_or(bytes),
        Err(_) => bytes,
    }
}
