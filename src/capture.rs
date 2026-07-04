use http::{header, HeaderMap};
use http_body_util::{BodyExt, Full};
use hudsucker::Body;

use crate::flow::{BodyRec, NotCapturedReason};

const STREAMING_CONTENT_TYPES: &[&str] = &["text/event-stream"];

fn streaming_content_type(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
    let base = raw.split(';').next().unwrap_or(raw).trim();
    STREAMING_CONTENT_TYPES.iter().any(|s| base.eq_ignore_ascii_case(s)).then(|| raw.to_string())
}

/// Drain a body up to `cap`, decide how it was captured, and return a fresh body
/// to forward — the read is destructive, so the body is always rebuilt, even on
/// the cap-exceeded path, so a capture decision never breaks the connection.
///
/// Streaming content types (SSE and friends) are forwarded untouched: a body
/// that never ends must never be buffered, or the exchange stalls forever.
pub async fn capture(headers: &HeaderMap, body: Body, cap: usize) -> (BodyRec, Body) {
    if let Some(content_type) = streaming_content_type(headers) {
        return (BodyRec::Streaming { content_type }, body);
    }

    match body.collect().await {
        Ok(collected) => {
            let bytes = collected.to_bytes();
            let size = bytes.len();
            let rec = if size == 0 {
                BodyRec::Empty
            } else if size > cap {
                BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual: size } }
            } else {
                BodyRec::Captured { bytes: bytes.clone(), size }
            };
            (rec, Body::from(Full::new(bytes)))
        }
        Err(_) => (BodyRec::NotCaptured { reason: NotCapturedReason::ReadError }, Body::empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::{capture, streaming_content_type};
    use crate::flow::{BodyRec, NotCapturedReason};
    use bytes::Bytes;
    use http::{header, HeaderMap};
    use http_body_util::{BodyExt, Full};
    use hudsucker::Body;

    fn body(data: &[u8]) -> Body {
        Body::from(Full::new(Bytes::copy_from_slice(data)))
    }

    async fn drain(b: Body) -> Vec<u8> {
        b.collect().await.expect("collect forwarded body").to_bytes().to_vec()
    }

    fn ct(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, value.parse().unwrap());
        headers
    }

    #[tokio::test]
    async fn empty_body_is_empty() {
        let (rec, fwd) = capture(&HeaderMap::new(), body(b""), 100).await;
        assert!(matches!(rec, BodyRec::Empty));
        assert!(drain(fwd).await.is_empty());
    }

    #[tokio::test]
    async fn within_cap_captured_and_forwarded_intact() {
        let (rec, fwd) = capture(&HeaderMap::new(), body(b"hello"), 100).await;
        match rec {
            BodyRec::Captured { size, bytes } => {
                assert_eq!(size, 5);
                assert_eq!(&bytes[..], b"hello");
            }
            other => panic!("expected Captured, got {other:?}"),
        }
        assert_eq!(drain(fwd).await, b"hello");
    }

    #[tokio::test]
    async fn over_cap_not_captured_but_forwarded_in_full() {
        let data = vec![7u8; 50];
        let (rec, fwd) = capture(&HeaderMap::new(), body(&data), 10).await;
        match rec {
            BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual } } => {
                assert_eq!(actual, 50);
            }
            other => panic!("expected TooLarge, got {other:?}"),
        }
        assert_eq!(drain(fwd).await, data);
    }

    #[tokio::test]
    async fn streaming_forwarded_untouched() {
        let (rec, fwd) = capture(&ct("text/event-stream"), body(b"data: x\n\n"), 100).await;
        assert!(matches!(rec, BodyRec::Streaming { .. }));
        assert_eq!(drain(fwd).await, b"data: x\n\n");
    }

    #[test]
    fn streaming_detected_with_params_and_case() {
        assert!(streaming_content_type(&ct("text/event-stream; charset=utf-8")).is_some());
        assert!(streaming_content_type(&ct("TEXT/EVENT-STREAM")).is_some());
        assert!(streaming_content_type(&ct("application/json")).is_none());
        assert!(streaming_content_type(&HeaderMap::new()).is_none());
    }

    proptest::proptest! {
        #[test]
        fn byte_round_trip(data in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..2000), cap in 0usize..1500) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let (rec, fwd) = rt.block_on(capture(&HeaderMap::new(), body(&data), cap));
            let forwarded = rt.block_on(drain(fwd));
            proptest::prop_assert_eq!(&forwarded, &data);
            match rec {
                BodyRec::Empty => proptest::prop_assert!(data.is_empty()),
                BodyRec::Captured { size, bytes } => {
                    proptest::prop_assert!(size <= cap && size == data.len());
                    proptest::prop_assert_eq!(&bytes[..], &data[..]);
                }
                BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual } } => {
                    proptest::prop_assert!(data.len() > cap);
                    proptest::prop_assert_eq!(actual, data.len());
                }
                other => proptest::prop_assert!(false, "unexpected {:?}", other),
            }
        }
    }
}
