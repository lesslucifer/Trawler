# Trawler

A native HTTP(S) debugging proxy written in Rust — a lightweight, hackable take on
[Charles](https://www.charlesproxy.com/) and [Proxyman](https://proxyman.io/). Trawler
sits between your apps and the network, decrypts HTTPS via a locally-trusted CA, and
logs every request and response flowing through it.

> ⚠️ **For debugging your own traffic only.** Intercepting TLS traffic requires
> installing a custom root CA. Only run this against devices and services you own or
> are authorized to inspect.

## Features

- **HTTP & HTTPS interception** — transparent MITM proxy with on-the-fly certificate generation.
- **Self-managed CA** — generates and persists a local Certificate Authority on first run.
- **Structured logging** — every request/response is logged via `tracing`.
- Built on [`hudsucker`](https://crates.io/crates/hudsucker), `tokio`, `hyper`, and `rustls`.

## Roadmap

- [ ] Request/response body capture and pretty-printing (JSON, forms, etc.)
- [ ] Persistent session storage and replay
- [ ] Filtering and search
- [ ] Breakpoints / request rewriting
- [ ] TUI or web dashboard
- [ ] WebSocket frame inspection

## Getting started

### Prerequisites

- [Rust](https://rustup.rs/) (stable, 2021 edition or later)

### Build & run

```bash
cargo run --release
```

Trawler starts a proxy on `http://127.0.0.1:3000` and writes a CA to `./ca/` on first run.

### Point traffic at the proxy

Configure your OS, browser, or device to use `127.0.0.1:3000` as its HTTP/HTTPS proxy.

**macOS example:**

```bash
networksetup -setwebproxy "Wi-Fi" 127.0.0.1 3000
networksetup -setsecurewebproxy "Wi-Fi" 127.0.0.1 3000
# to disable later:
networksetup -setwebproxystate "Wi-Fi" off
networksetup -setsecurewebproxystate "Wi-Fi" off
```

### Trust the CA (for HTTPS)

To decrypt HTTPS, the generated CA at `ca/trawler-ca.pem` must be trusted by your client.

**macOS:**

```bash
sudo security add-trusted-cert -d -r trustRoot \
  -k /Library/Keychains/System.keychain ca/trawler-ca.pem
```

Adjust filter verbosity with `RUST_LOG`, e.g. `RUST_LOG=trawler=debug cargo run`.

## Project layout

```
src/main.rs   Proxy bootstrap, CA management, and logging handler
ca/            Generated CA (git-ignored) — do not commit or share the private key
```

## License

MIT
