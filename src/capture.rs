use bytes::{Bytes, BytesMut};
use http::{header, HeaderMap};
use http_body_util::{BodyStream, Full, StreamBody};
use hudsucker::hyper::body::Frame;
use hudsucker::Body;
use tokio_stream::StreamExt;

use crate::flow::{BodyRec, NotCapturedReason};

const STREAMING_CONTENT_TYPES: &[&str] = &["text/event-stream"];

fn streaming_content_type(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
    let base = raw.split(';').next().unwrap_or(raw).trim();
    STREAMING_CONTENT_TYPES.iter().any(|s| base.eq_ignore_ascii_case(s)).then(|| raw.to_string())
}

/// Drain a body frame by frame up to `cap`, decide how it was captured, and
/// return a fresh body to forward — the read is destructive, so buffered
/// frames are always replayed into the forwarded body and a capture decision
/// never breaks the connection.
///
/// The buffer is bounded: the moment `cap` is exceeded the decision is made
/// and the remaining frames pass through untouched, so an oversized or
/// hostile body can never force more than cap-plus-one-frame into memory.
/// `TooLarge::actual` is the declared `content-length` when one exists,
/// otherwise the bytes seen when counting stopped.
///
/// Streaming content types (SSE and friends) are forwarded untouched: a body
/// that never ends must never be buffered, or the exchange stalls forever.
pub async fn capture(headers: &HeaderMap, body: Body, cap: usize) -> (BodyRec, Body) {
    if let Some(content_type) = streaming_content_type(headers) {
        return (BodyRec::Streaming { content_type }, body);
    }

    let mut stream = BodyStream::new(body);
    let mut frames: Vec<Frame<Bytes>> = Vec::new();
    let mut size = 0usize;

    while let Some(result) = stream.next().await {
        let Ok(frame) = result else {
            return (BodyRec::NotCaptured { reason: NotCapturedReason::ReadError }, Body::empty());
        };
        size += frame.data_ref().map_or(0, Bytes::len);
        frames.push(frame);

        if size > cap {
            let actual = declared_length(headers).map_or(size, |declared| declared.max(size));
            let rest = tokio_stream::iter(frames.into_iter().map(Ok)).chain(stream);
            return (
                BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual } },
                Body::from(StreamBody::new(rest)),
            );
        }
    }

    if size == 0 {
        return (BodyRec::Empty, Body::from(Full::new(Bytes::new())));
    }
    let bytes = contiguous(frames, size);
    (BodyRec::Captured { bytes: bytes.clone(), size }, Body::from(Full::new(bytes)))
}

fn contiguous(mut frames: Vec<Frame<Bytes>>, size: usize) -> Bytes {
    if frames.len() == 1 {
        return frames.remove(0).into_data().unwrap_or_default();
    }
    let mut buf = BytesMut::with_capacity(size);
    for frame in &frames {
        if let Some(data) = frame.data_ref() {
            buf.extend_from_slice(data);
        }
    }
    buf.freeze()
}

fn declared_length(headers: &HeaderMap) -> Option<usize> {
    headers.get(header::CONTENT_LENGTH)?.to_str().ok()?.parse().ok()
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
        assert_eq!(drain(fwd).await.len(), 0);
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
    async fn over_cap_decided_without_waiting_for_body_end() {
        let never_ending = tokio_stream::StreamExt::chain(
            tokio_stream::iter(vec![Ok::<Bytes, std::io::Error>(Bytes::from(vec![1u8; 20]))]),
            tokio_stream::pending(),
        );
        let (rec, _fwd) = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            capture(&HeaderMap::new(), Body::from_stream(never_ending), 10),
        )
        .await
        .expect("capture must decide once the cap is exceeded, not wait for body end");
        assert!(matches!(
            rec,
            BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual: 20 } }
        ));
    }

    #[tokio::test]
    async fn multi_frame_over_cap_forwarded_in_full() {
        let frames: Vec<Result<Bytes, std::io::Error>> =
            (0..3).map(|_| Ok(Bytes::from(vec![7u8; 5]))).collect();
        let body = Body::from_stream(tokio_stream::iter(frames));
        let (rec, fwd) = capture(&HeaderMap::new(), body, 8).await;
        match rec {
            BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual } } => {
                assert_eq!(actual, 10, "counting stops at the frame that crossed the cap");
            }
            other => panic!("expected TooLarge, got {other:?}"),
        }
        assert_eq!(drain(fwd).await, vec![7u8; 15], "full body must still reach the client");
    }

    #[tokio::test]
    async fn over_cap_actual_prefers_declared_content_length() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, "999".parse().unwrap());
        let (rec, _fwd) = capture(&headers, body(&[7u8; 50]), 10).await;
        assert!(matches!(
            rec,
            BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual: 999 } }
        ));
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
