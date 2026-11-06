# Independent review checklists (router-run, read-only, Opus)

Spawn: Agent, `subagent_type: Explore`, `model: opus`, prompt from `router-agent/references/messages.md` (say what earlier rounds covered). Hunts that found real defects are marked ★.

**A. Shell / quit / lifecycle** (`ui/src-tauri/src/lifecycle.rs`, menu.rs, tray.rs, lib.rs)
- ★ Every `app.exit`/`quit_app` path runs DEC-64's system-proxy check (found: failed window reopen exited; stale quit flag quit later).
- Window reopen failure, mid-destroy windows, stale quit flags; races ✕ vs ⌘Q vs tray vs Dock Quit, double requests, failed `take_quit_request`.
- Windowless states with keep-capturing on/off and no tray host (Linux); never unreachable. Second launch (`--background` skip) still reopens the window.
- Updater install goes through the same quit request (DEC-82); restart args; dropped updates.

**B. `unsafe` / native** (`login-service/`, DEC-84)
- ★ `unsafe` only in the one module; guard test catches `#![allow(unsafe_code, ...)]` and item-level allows.
- Each ObjC call: lifetimes, null, thread, error mapping, no panic across FFI.
- ★ SMAppService is macOS 13+, the app's minimum is 11 → runtime guard (found: lookup would abort). Unbundled `cargo run` refuses with "unavailable"; DMG/translocated paths.

**C. Signing key / updater** (DEC-82, DEC-86 rule 4)
- ★ Secrets by name, only in the `--sign-only` step; presence check is a boolean; no pnpm/npm install, lifecycle script or bundler-downloaded tool beside the key (found: AppImage downloads; pnpm 11 verify-deps-before-run).
- `requireSignedVersion`: version in each signature; tampered archive refused.
- Updater: TLS/redirects, temp files, deb/rpm root install via pkexec (upstream TOCTOU recorded), command injection.

**D. Release / CI workflows** (`.github/workflows/*.yml`, `xtask` release code)
- Trigger + tag == version; `${{ }}` of refs/tags only through `env:`.
- Every `uses:` pinned to a full SHA matching its tag; no caches; `persist-credentials: false`.
- ★ `pipefail` with `| grep -q` / `| head` → exit 141: capture then compare.
- Runner labels exist; arm64 rows build arm64; `fail-fast: false` for first runs.
- Artifact names/flattening match `release-manifest`; collisions fail loudly.
- ★ Version accepted by every bundler: WiX numeric pre-release, rpm no `-`, deb, NSIS, macOS; verify in tauri-bundler source at the pinned tag. `actionlint` clean.

**E. Tests that could touch the owner's machine** (`ui/e2e`, `xtask/tests/package_bundle.rs`)
- ★ HOME/config dir is a temp dir, asserted before any delete/write (found: a spec deleting the real `keep-capturing.json`).
- ★ Ports 7420/7421/4445 and the single-instance socket: refuse when the owner's Trawler is up.
- Every failure path detaches mounts (`hdiutil detach`), kills only its own pids (no broad `pgrep -f`), leaves nothing listening.
- Specs independent of leftover state; wait instead of immediate reads (`/flows`).

**F. Back-merges**: ★ per conflicted file compare both parents (found: a dropped guard); duplicated rows (decisions index); message claims match content.
