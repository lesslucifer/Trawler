# Contributing to Trawler

Thank you for taking the time to help.

## Issues, not pull requests (yet)

Bug reports and ideas are very welcome as [issues](https://github.com/lesslucifer/Trawler/issues/new/choose). Pull requests aren't open yet; this may change in a later release.

What helps most in a report:

- your Trawler version (**Settings → General**) and your system with its version,
- how you installed it (`.dmg`, `-setup.exe`, `.msi`, AppImage, `.deb`, `.rpm`, or from source),
- the steps that show the problem, what you expected, and what happened.

Remove tokens, cookies and private hosts from anything you paste. A security problem goes to [SECURITY.md](SECURITY.md), never to a public issue.

## Build from source

You need [Rust](https://rustup.rs/) (stable), Node.js and [pnpm](https://pnpm.io/).

```sh
cargo xtask hooks install        # once per clone: the repo's git hooks
cargo run                        # the app, with its own capture core
cargo run -p trawler             # the capture core alone: proxy on :7420, API on :7421
RUST_LOG=trawler=debug cargo run -p trawler
cargo test                       # everything; -p trawler for the core only
cargo clippy
cargo xtask ui dev               # the window with hot reload
cargo xtask ui e2e               # WebDriver tests against the real window
cargo xtask ui screenshots       # the README and guide screenshots
cargo xtask package              # installers for this system
```

Quick check with the core running:

```sh
curl -x http://127.0.0.1:7420 http://example.com/    # send a request through Trawler
curl http://127.0.0.1:7421/flows                      # list what it captured
```

## Project layout

```
src/            the capture core (library + thin binary)
  lib.rs        composition root: run(Config) wires the proxy and the control API
  config.rs     runtime configuration and defaults
  flow.rs       domain model: Flow, RequestRec/ResponseRec, BodyRec
  capture.rs    body drain, the 10 MB cap, the streaming rule
  store.rs      byte-weighted ring buffer, broadcast bus, filtering
  recorder.rs   the proxy handler that produces flows
  ca.rs         certificate authority material
  api/          the control API: routes, views, decoding, HAR
ui/             the app: React + Tailwind front end
ui/src-tauri/   the app's native shell (Tauri): windows, menus, setup, updates
login-service/  the macOS open-at-login helper
xtask/          build, test, package and release tooling
tests/          core integration and end-to-end tests
docs/           design and process documents (HTML)
```

How it fits together: [architecture](docs/architecture.html) · why it is built this way: [core technical design](docs/core-technical-design.html).

## Control API

The core listens on `127.0.0.1:7421`. Every route answers JSON unless noted.

| Endpoint | Does |
|---|---|
| `GET /flows` | captured requests, newest first; filter with `host`, `method`, `status`, `content_type`, `q`, `source`, `is` (prefix `not_` to leave out), page with `limit` and `cursor` |
| `POST /flows` | send a new request; it lands as a flow |
| `DELETE /flows` | clear the flows the same filter matches |
| `GET /flows/{id}` | one flow in full: headers, timing, body states |
| `DELETE /flows/{id}` | remove one flow |
| `POST /flows/{id}/replay` | send a flow again, optionally edited |
| `GET /flows/{id}/request/body` · `…/response/body` | raw bytes; `?decode=1` reverses `content-encoding` |
| `GET /flows/export` | the filtered flows as HAR |
| `POST /flows/import` | load a HAR document |
| `GET /stream` | live Server-Sent Events of flow changes (no bodies) |
| `GET /stats` | how many connections are open |
| `GET` · `POST` · `DELETE /reach` | whether another device may reach Trawler; allow one address; stop |
| `GET /ca.crt` · `/ca.pem` · `/ca.cer` · `/ca.mobileconfig` | Trawler's certificate in each format a system installs |

## How changes land

- Every change carries its tests: [testing strategy](docs/testing-strategy.html). Tests are hermetic: local servers, ephemeral ports, never the public network.
- Lints: clippy pedantic; `unsafe` code is forbidden.
- The maintainer takes each change through the project's gates before it merges: [workflow](docs/workflow.html).
