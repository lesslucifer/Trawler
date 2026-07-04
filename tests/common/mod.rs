#![allow(dead_code)]

use std::io::Write;

use bytes::Bytes;
use flate2::write::GzEncoder;
use flate2::Compression;
use trawler::flow::{BodyRec, Flow, Header, RequestRec, ResponseRec};

pub fn captured(data: &[u8]) -> BodyRec {
    BodyRec::Captured { bytes: Bytes::copy_from_slice(data), size: data.len() }
}

pub fn header(name: &str, value: &str) -> Header {
    Header { name: name.into(), value: value.into() }
}

pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("gzip write");
    encoder.finish().expect("gzip finish")
}

/// Same fluent shape as the crate-internal builder, exposed for the external
/// integration test crates (which cannot see `#[cfg(test)]` items).
pub struct FlowBuilder {
    method: String,
    uri: String,
    req_headers: Vec<Header>,
    req_body: BodyRec,
    response: Option<ResponseRec>,
    duration_ms: Option<u64>,
}

impl FlowBuilder {
    pub fn new() -> Self {
        Self {
            method: "GET".into(),
            uri: "http://example.com/".into(),
            req_headers: Vec::new(),
            req_body: BodyRec::Empty,
            response: None,
            duration_ms: None,
        }
    }

    pub fn method(mut self, method: &str) -> Self {
        self.method = method.into();
        self
    }

    pub fn uri(mut self, uri: &str) -> Self {
        self.uri = uri.into();
        self
    }

    pub fn req_header(mut self, name: &str, value: &str) -> Self {
        self.req_headers.push(header(name, value));
        self
    }

    pub fn req_body(mut self, body: BodyRec) -> Self {
        self.req_body = body;
        self
    }

    pub fn status(mut self, status: u16) -> Self {
        self.response_mut().status = status;
        self
    }

    pub fn resp_header(mut self, name: &str, value: &str) -> Self {
        self.response_mut().headers.push(header(name, value));
        self
    }

    pub fn resp_body(mut self, body: BodyRec) -> Self {
        self.response_mut().body = body;
        self
    }

    fn response_mut(&mut self) -> &mut ResponseRec {
        self.duration_ms.get_or_insert(1);
        self.response.get_or_insert_with(|| ResponseRec {
            status: 200,
            version: "HTTP/1.1".into(),
            headers: Vec::new(),
            body: BodyRec::Empty,
        })
    }

    pub fn build(self) -> Flow {
        let mut flow = Flow::new(RequestRec {
            method: self.method,
            uri: self.uri,
            version: "HTTP/1.1".into(),
            headers: self.req_headers,
            body: self.req_body,
        });
        flow.response = self.response;
        flow.duration_ms = self.duration_ms;
        flow
    }
}

impl Default for FlowBuilder {
    fn default() -> Self {
        Self::new()
    }
}
