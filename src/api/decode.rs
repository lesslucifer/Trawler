use std::io::Read;

use flate2::read::{GzDecoder, ZlibDecoder};

/// Zip-bomb guard: stored bodies are capped at `body_cap`, but their decoded
/// form is not — without this bound a high-ratio payload expands without limit.
const MAX_DECODED_LEN: usize = 64 * 1024 * 1024;

/// View-time decode chain, run lazily only for `?decode=1`: reverse
/// `content-encoding`, transcode a non-UTF-8 charset to UTF-8, then pretty-print
/// if the payload is JSON or XML. Capture stays fast and honest — raw bytes are
/// what's stored; this is a rendering, and any step falls back to its input.
pub fn decode_body(
    raw: &[u8],
    content_encoding: Option<&str>,
    content_type: Option<&str>,
) -> Vec<u8> {
    let utf8 = to_utf8(decompress_body(raw, content_encoding), content_type);

    if is_json(content_type) {
        pretty_json(utf8)
    } else if is_xml(content_type) {
        pretty_xml(utf8)
    } else {
        utf8
    }
}

/// Transcode to UTF-8 when the content type names a non-UTF-8 charset. Absent or
/// UTF-8/ASCII charsets pass through untouched; an unknown label is left as-is.
fn to_utf8(bytes: Vec<u8>, content_type: Option<&str>) -> Vec<u8> {
    let Some(label) = charset(content_type) else { return bytes };
    let Some(encoding) = encoding_rs::Encoding::for_label(label.as_bytes()) else { return bytes };
    if encoding == encoding_rs::UTF_8 {
        return bytes;
    }
    let (decoded, _, _) = encoding.decode(&bytes);
    decoded.into_owned().into_bytes()
}

fn charset(content_type: Option<&str>) -> Option<String> {
    content_type?
        .split(';')
        .filter_map(|param| param.split_once('='))
        .find(|(key, _)| key.trim().eq_ignore_ascii_case("charset"))
        .map(|(_, value)| value.trim().trim_matches('"').to_ascii_lowercase())
}

fn is_xml(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|ct| {
        let ct = ct.to_ascii_lowercase();
        ct.contains("xml") || ct.contains("html")
    })
}

fn pretty_xml(bytes: Vec<u8>) -> Vec<u8> {
    use quick_xml::events::Event;
    use quick_xml::{Reader, Writer};

    let mut reader = Reader::from_reader(&bytes[..]);
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    let mut depth = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(event) => {
                match event {
                    Event::Start(_) => depth += 1,
                    Event::End(_) => depth = depth.saturating_sub(1),
                    _ => {}
                }
                if writer.write_event(event).is_err() {
                    return bytes;
                }
            }
            Err(_) => return bytes,
        }
    }
    if depth != 0 {
        return bytes;
    }
    writer.into_inner()
}

/// The decode chain's first step alone, for `?decode=source`: a compressed body
/// is unreadable as stored, but reversing the transport encoding is not the same
/// as re-rendering the payload — the bytes that come back are what the server
/// actually sent, unindented and untranscoded.
pub fn decompress_body(raw: &[u8], content_encoding: Option<&str>) -> Vec<u8> {
    match content_encoding.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("gzip" | "x-gzip") => decompress(GzDecoder::new(raw), raw, MAX_DECODED_LEN),
        Some("deflate") => decompress(ZlibDecoder::new(raw), raw, MAX_DECODED_LEN),
        Some("br") => decompress(brotli::Decompressor::new(raw, 4096), raw, MAX_DECODED_LEN),
        _ => raw.to_vec(),
    }
}

fn decompress<R: Read>(decoder: R, raw: &[u8], max: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let limit = u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1);
    match decoder.take(limit).read_to_end(&mut out) {
        Ok(_) if out.len() <= max => out,
        _ => raw.to_vec(),
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
    use super::{decode_body, decompress};
    use crate::test_support::{brotli, gzip, zlib};
    use flate2::read::GzDecoder;

    #[test]
    fn zip_bomb_over_limit_falls_back_to_raw() {
        let bomb = gzip(&vec![0u8; 100_000]);
        assert_eq!(decompress(GzDecoder::new(&bomb[..]), &bomb, 1_000), bomb);
    }

    #[test]
    fn decoded_exactly_at_limit_passes() {
        let data = vec![0u8; 1_000];
        assert_eq!(decompress(GzDecoder::new(&gzip(&data)[..]), &gzip(&data), 1_000), data);
    }

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
        assert_eq!(decode_body(raw, Some("identity"), None), raw);
        assert_eq!(decode_body(raw, Some("nope"), None), raw);
    }

    #[test]
    fn brotli_round_trip_and_alias() {
        let raw = b"brotli me brotli me brotli me";
        assert_eq!(decode_body(&brotli(raw), Some("br"), None), raw);
        assert_eq!(decode_body(&brotli(raw), Some("BR"), None), raw);
        assert_eq!(decode_body(&brotli(raw), Some(" br "), None), raw);
    }

    #[test]
    fn corrupt_brotli_falls_back_to_raw() {
        let junk = b"not really brotli at all";
        assert_eq!(decode_body(junk, Some("br"), None), junk);
    }

    #[test]
    fn charset_transcoded_to_utf8() {
        let latin1 = *b"caf\xe9"; // café in ISO-8859-1
        let out = decode_body(&latin1, None, Some("text/plain; charset=ISO-8859-1"));
        assert_eq!(String::from_utf8(out).unwrap(), "café");
    }

    #[test]
    fn charset_shift_jis_transcoded() {
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode("日本語");
        let out = decode_body(&bytes, None, Some("text/plain; charset=Shift_JIS"));
        assert_eq!(String::from_utf8(out).unwrap(), "日本語");
    }

    #[test]
    fn utf8_and_unknown_charset_pass_through() {
        let utf8 = "café".as_bytes();
        assert_eq!(decode_body(utf8, None, Some("text/plain; charset=utf-8")), utf8);
        assert_eq!(decode_body(utf8, None, Some("text/plain; charset=nonsense")), utf8);
        assert_eq!(decode_body(utf8, None, Some("text/plain")), utf8);
    }

    #[test]
    fn xml_pretty_printed() {
        let compact = b"<a><b>1</b><c>2</c></a>";
        let out = decode_body(compact, None, Some("application/xml"));
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains('\n'), "expected indentation: {text}");
        assert!(text.contains("  <b>1</b>"), "expected nested indent: {text}");
    }

    #[test]
    fn html_content_type_pretty_printed() {
        let compact = b"<div><p>hi</p></div>";
        let out = decode_body(compact, None, Some("text/html"));
        assert!(out.contains(&b'\n'));
    }

    #[test]
    fn malformed_xml_unchanged() {
        let bad = b"<a><b>unclosed";
        assert_eq!(decode_body(bad, None, Some("application/xml")), bad);
    }

    #[test]
    fn brotli_charset_then_xml_chain() {
        let (latin1, _, _) = encoding_rs::WINDOWS_1252.encode("<r><v>é</v></r>");
        let out = decode_body(
            &brotli(&latin1),
            Some("br"),
            Some("application/xml; charset=windows-1252"),
        );
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("é"), "charset applied: {text}");
        assert!(text.contains('\n'), "xml pretty applied: {text}");
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
        fn brotli_lossless(data in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..500)) {
            proptest::prop_assert_eq!(decode_body(&brotli(&data), Some("br"), None), data);
        }

        #[test]
        fn junk_passes_through_and_never_panics(data in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..500)) {
            proptest::prop_assert_eq!(decode_body(&data, Some("nope"), None), data.clone());
            let _ = decode_body(&data, Some("gzip"), Some("application/json"));
            let _ = decode_body(&data, Some("deflate"), None);
        }
    }
}
