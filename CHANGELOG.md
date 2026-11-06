# Changelog

Each release's notes open with its section here. A pre-release (`0.1.0-5`) uses its final's section (`## 0.1.0`); `cargo xtask release-manifest` refuses a tag whose section is missing or empty.

## 0.1.0

The first public release.

- Capture HTTP and HTTPS from this machine or another device through a local proxy and a CA generated on first run.
- Guided setup per platform (macOS, Windows, Linux, iOS, Android, the iOS Simulator) that proves capture with a real request.
- A live, filterable flow list: typed filter chips such as `status:4xx` or `-method:OPTIONS`.
- An inspector with headers, timing, per-leg HTTP versions and body viewers chosen by what the bytes are.
- Compose and replay requests; copy any flow as curl.
- Export and import HAR.
- Keep capturing with the window closed, from the menu-bar or tray icon, and open at login.
- In-app updates: a signed update is offered at launch and from Check for Updates….
- Installers for macOS (universal), Windows and Linux, x64 and arm64.
