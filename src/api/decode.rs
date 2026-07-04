use std::io::Read;

use flate2::read::{GzDecoder, ZlibDecoder};

/// View-time decode chain, run lazily only for `?decode=1`: reverse
/// `content-encoding`, then pretty-print if the payload is JSON. Capture stays
/// fast and honest — raw bytes are what's stored; this is a rendering.
pub fn decode_body(
    raw: &[u8],
    content_encoding: Option<&str>,
    content_type: Option<&str>,
) -> Vec<u8> {
    let decompressed = match content_encoding.map(str::trim).map(str::to_ascii_lowercase).as_deref()
    {
        Some("gzip" | "x-gzip") => decompress(GzDecoder::new(raw), raw),
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
    content_type.is_some_and(|ct| ct.to_ascii_lowercase().contains("json"))
}

fn pretty_json(bytes: Vec<u8>) -> Vec<u8> {
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(value) => serde_json::to_vec_pretty(&value).unwrap_or(bytes),
        Err(_) => bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::decode_body;
    use crate::test_support::{gzip, zlib};

    #[test]
    fn gzip_round_trip_and_aliases() {
        let raw = b"hello world hello world";
        assert_eq!(decode_body(&gzip(raw), Some("gzip"), None), raw);
        assert_eq!(decode_body(&gzip(raw), Some("x-gzip"), None), raw);
        assert_eq!(decode_body(&gzip(raw), Some("GZIP"), None), raw);
    }

    #[test]
    fn deflate_round_trip() {
        let raw = b"deflate me deflate me";
        assert_eq!(decode_body(&zlib(raw), Some("deflate"), None), raw);
    }

    #[test]
    fn unknown_or_absent_encoding_is_identity() {
        let raw = b"plain bytes";
        assert_eq!(decode_body(raw, None, None), raw);
        assert_eq!(decode_body(raw, Some("br"), None), raw);
        assert_eq!(decode_body(raw, Some("identity"), None), raw);
    }

    #[test]
    fn corrupt_gzip_falls_back_to_raw() {
        let junk = b"\x1f\x8b not really gzip";
        assert_eq!(decode_body(junk, Some("gzip"), None), junk);
    }

    #[test]
    fn json_pretty_only_for_json_content_type() {
        let compact = br#"{"a":1,"b":2}"#;
        let pretty = decode_body(compact, None, Some("application/json"));
        assert!(pretty.contains(&b'\n'));
        assert!(pretty.len() > compact.len());
        assert_eq!(decode_body(compact, None, Some("text/plain")), compact);
    }

    #[test]
    fn invalid_json_unchanged() {
        let bad = b"{not json";
        assert_eq!(decode_body(bad, None, Some("application/json")), bad);
    }

    #[test]
    fn gzip_then_json_pretty() {
        let compact = br#"{"a":1}"#;
        let out = decode_body(&gzip(compact), Some("gzip"), Some("application/JSON"));
        assert!(out.len() > compact.len());
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["a"], 1);
    }

    proptest::proptest! {
        #[test]
        fn gzip_lossless(data in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..500)) {
            proptest::prop_assert_eq!(decode_body(&gzip(&data), Some("gzip"), None), data);
        }

        #[test]
        fn deflate_lossless(data in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..500)) {
            proptest::prop_assert_eq!(decode_body(&zlib(&data), Some("deflate"), None), data);
        }

        #[test]
        fn junk_passes_through_and_never_panics(data in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..500)) {
            proptest::prop_assert_eq!(decode_body(&data, Some("nope"), None), data.clone());
            let _ = decode_body(&data, Some("gzip"), Some("application/json"));
            let _ = decode_body(&data, Some("deflate"), None);
        }
    }
}
