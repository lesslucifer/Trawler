# Trawler

A native HTTP(S) debugging proxy written in Rust — a lightweight, hackable take on
[Charles](https://www.charlesproxy.com/) and [Proxyman](https://proxyman.io/). Trawler
sits between your apps and the network, decrypts HTTPS via a locally-trusted CA, and
captures every request and response flowing through it — exposed over a local,
scriptable control API.

> ⚠️ **For debugging your own traffic only.** Intercepting TLS traffic requires
> installing a custom root CA. Only run this against devices and services you own or
> are authorized to inspect.

## Features

- **HTTP & HTTPS interception** — MITM proxy with on-the-fly per-host certificate generation ([`hudsucker`](https://crates.io/crates/hudsucker) 0.24).
- **Self-managed CA** — generates and persists a local Certificate Authority on first run; downloadable at `/ca.crt`.
- **Body capture** — buffer-then-forward up to a 10 MB per-request cap, with a streaming (SSE) fast-path that is never buffered.
- **In-memory store** — a byte-weighted ring buffer of live flows; nothing hits disk.
- **Control API** — inspect-only REST + a live Server-Sent-Events stream, scriptable from `curl` on day one.
- **View-side decoding** — `?decode=1` reverses `content-encoding` (gzip/deflate) and pretty-prints JSON, leaving stored bytes untouched.

## Architecture

- **[Core Technical Design](docs/core-technical-design.html)** — the *why*: capture model, body-cap rule, TLS interception, CA/trust, and the hard limits.
- **[Core Architecture](docs/core-architecture.html)** — the *how*: crate shape, modules, the request/response lifecycle in code, and the concurrency model.

The proxy handler is a dumb producer; a central store owns the canonical flows; every
interface is a subscriber over a broadcast bus. The core is a library (`src/lib.rs`);
the binary (`src/main.rs`) is a thin launcher — any UI is just a client of the API.

## Getting started

### Prerequisites

- [Rust](https://rustup.rs/) (stable, 2021 edition or later)

### Run

```sh
cargo run                         # proxy on :7420, control API on :7421
RUST_LOG=trawler=debug cargo run  # verbose
```

Then point a client at the proxy and watch traffic land in the API:

```sh
curl -x http://127.0.0.1:7420 http://example.com/     # proxy a request
curl http://127.0.0.1:7421/flows                       # list captured flows
curl -N http://127.0.0.1:7421/stream                   # live SSE of new flows
```

For HTTPS, download and trust the CA (`http://127.0.0.1:7421/ca.crt`) — see the
per-platform routing/trust table in the [Core Technical Design](docs/core-technical-design.html#network).

## Control API

| Endpoint | Returns |
|---|---|
| `GET /flows?host=&method=&status=&content_type=&q=` | flow summaries, newest first |
| `GET /flows/{id}` | full flow detail (headers + body states) |
| `GET /flows/{id}/request/body` · `…/response/body` | raw bytes; `?decode=1` for decoded |
| `GET /stream` | live SSE lifecycle events (compact, no bodies) |
| `GET /ca.crt` | root CA certificate (PEM) |

## Roadmap

- [ ] Persistent session storage (SQLite / HAR) behind the store boundary
- [ ] Replay and breakpoints / request rewriting
- [ ] A TUI or web dashboard client
- [ ] WebSocket frame inspection
- [ ] Transparent (NAT) interception

## Project layout

```
src/lib.rs      Composition root — run(Config) wires proxy + control API
src/main.rs     Thin binary launcher
src/config.rs   Runtime configuration and defaults
src/flow.rs     Domain model: Flow, RequestRec/ResponseRec, BodyRec
src/event.rs    FlowEvent (broadcast lifecycle events)
src/capture.rs  Body drain / cap / streaming rule
src/store.rs    Byte-weighted ring buffer + broadcast bus + filtering
src/recorder.rs hudsucker HttpHandler — the capture producer
src/ca.rs       Certificate authority material
src/api/        axum control API: routes, view projections, decode chain
docs/           Interactive HTML design & architecture docs
ca/             Generated CA (git-ignored) — never commit or share the private key
```
