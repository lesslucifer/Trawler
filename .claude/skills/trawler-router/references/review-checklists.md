# Independent review checklists (router-run, read-only, Opus)

Spawn: Agent, `subagent_type: Explore`, `model: opus`, description "Opus review <what>".
Common header: worktree, branch, tip, base, "do not edit, run mr, push, tag, or
execute anything beyond git/grep/read (read-only gh api ok)", scope as
`git diff <base> HEAD -- <paths>`, "report file:line, severity, CONFIRMED vs
PLAUSIBLE, concrete scenario; list what you checked clean; concise". Tell it what
earlier rounds already covered so it spends effort elsewhere.

## A. Shell / quit / lifecycle (`ui/src-tauri/src/lifecycle.rs`, menu.rs, tray.rs, lib.rs)
- Every path to `app.exit` / `quit_app` runs DEC-64's system-proxy check, or explain why not needed.
- Window reopen failure, mid-destroy windows, stale quit flags taken by the next window.
- Races: ✕ vs ⌘Q vs tray Quit vs Dock Quit; double requests; failed `take_quit_request`.
- Windowless states with keep-capturing on/off and with no tray host (Linux); app never left unreachable.
- Second launch (`--background` skip) still reopens the window on a normal launch.
- Updater install goes through the same quit request (DEC-82); restart args; dropped updates.

## B. `unsafe` and native APIs (`login-service/`, DEC-84)
- `unsafe` only in the one module; guard test catches `#![allow(unsafe_code, ...)]` and item-level allows.
- Each ObjC call: lifetimes, null, thread, error mapping, no panic across FFI.
- OS availability: SMAppService is macOS 13+; the app's minimum is macOS 11 → runtime guard.
- Unbundled builds (`cargo run`) refuse with the "unavailable" text; DMG/translocated paths.

## C. Signing key and updater (DEC-82, DEC-86 rule 4)
- Secrets referenced by name only, only in the sign step; presence check is a boolean.
- No pnpm/npm install, lifecycle script, or bundler-downloaded tool runs while the key is in the env.
- `requireSignedVersion`: version embedded in each signature; tampered archive refused.
- Updater: TLS/redirects, temp files, deb/rpm root install via pkexec (upstream TOCTOU recorded), command injection.

## D. Release / CI workflows (`.github/workflows/*.yml`, `xtask` release code)
- Trigger and tag == version check; `${{ }}` of refs/tags only through `env:`.
- Every `uses:` pinned to a full SHA (verify the SHA matches the tag); no caches; `persist-credentials: false`.
- `set -o pipefail` with `| grep -q` / `| head` → exit 141; capture then compare.
- Matrix runner labels exist; arm64 rows really build arm64; `fail-fast: false` for first runs.
- Artifact names and flattening match what `release-manifest` expects; collisions fail loudly.
- Version string accepted by every bundler: WiX (numeric pre-release), rpm (no `-`), deb, NSIS, macOS. Verify in tauri-bundler source at the pinned tag, not from memory.
- `actionlint` clean.

## E. Tests that could touch the owner's machine (`ui/e2e`, `xtask/tests/package_bundle.rs`)
- HOME / config dir overridden to a temp dir and asserted before any delete/write.
- Fixed ports 7420/7421/4445 and the single-instance socket: refuse to run when the owner's Trawler is up.
- Every failure path detaches mounts (`hdiutil detach`), kills its own processes only (by pid, not `pgrep -f` on broad patterns), leaves nothing listening.
- Specs independent of each other's leftover state; waits instead of immediate reads (`/flows`).

## F. Back-merges into an epic
- For each conflicted file compare both parents; look for dropped guards and duplicated rows (decisions index).
- Merge commit message claims match the content.
