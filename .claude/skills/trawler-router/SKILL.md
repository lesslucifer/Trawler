---
name: trawler-router
description: Run the router role on the Trawler project (Rust + Tauri MITM proxy, sibling worktrees ../Trawler-<slug>). Use when coordinating Trawler worker sessions across worktrees: staging mr commit / mr confirm / merge / push / tag commands for the owner, verifying gate notes (test/v2, doc/v1, cr/v5, design/v1, mr/v1, dod/v1), closing epics, running independent reviews of ui/src-tauri, login-service or .github workflows, spawning Opus workers with briefs, or watching CI and release runs. Load the general router-agent skill too.
---

# Trawler router

Load `router-agent` first; this skill adds Trawler's facts. Sources: `CLAUDE.md`,
`docs/workflow.html`, `docs/testing-strategy.html`, DEC-3 (in
`docs/decisions/workflow-gates.html`), DEC-64, 77, 81–86, and the router session
of 2026-11-05/06 (`router-agent/references/lessons.md`). *(inference)* marks rules not
ordered by the owner. Code wins: if a fact here disagrees with `xtask/` or the docs, trust
them and fix this skill.

## 1. Layout

| Thing | Value |
|---|---|
| Main checkout | the first row of `git worktree list` (`/Users/nguyenvanluong/Documents/proj/misc/Trawler`), on `main`: merge-only, the pre-commit hook refuses non-merge commits there (DEC-6) |
| Worktrees | siblings `../Trawler-<slug>`; one per epic, extra ones for parallel WUs (`../Trawler-relfix-inspect`) |
| Branches | `epic/<slug>` off main; `wu/<epic-slug>/<slug>` off its epic |
| Ids | epic `E-<CODE>` (E-BGSVC, E-INSPWIN2, E-PKG2, E-CI, E-RCFIX, E-RELFIX); WU `E-<CODE>.W<m>`, numeric only ("W2b" had to become W3) |
| Remote | `origin` = `git@github.com:lesslucifer/Trawler.git` (public) |
| Owner id for mr | `--as vuluu` (`cut -d' ' -f2 .mr-allowed`; the file is git-ignored and copied into each new worktree) |
| Ports | 7420 proxy · 7421 control API · 4445 embedded WebDriver (e2e) |

## 2. Who runs what

| Action | Who |
|---|---|
| Plans, code, tests, result reports, distillation, `cargo xtask notes write`, `preflight`, `verify-dod`, the code-review skill (cr/v5), the design-review skill (design/v1) | worker |
| Independent read-only review of code no gate binds | router (subagent, Opus) |
| `cargo xtask mr commit` / `mr confirm` (Touch ID / key) | **owner only**; router stages, never runs |
| `git merge --no-ff` into an epic or main | owner (the classifier denied the router's merge as "Merge Without Review"; the owner chose to merge themselves) |
| `git push`, tags, `gh workflow run`, `gh secret set`, making the repo public | owner |
| Killing a process a worker's kill was denied for | owner |
| Reading `gh` runs/logs, `actionlint` (`/opt/homebrew/bin/actionlint`), git state | router (read-only) |

Never write a gate note by hand or copy one between tips (the classifier denied a cr note copy; it is a bypass).

## 3. The gate system (what a tip must carry)

| Note | When | Written by |
|---|---|---|
| `test/v2`, `doc/v1` | every WU/epic tip | `cargo xtask notes write wu\|epic <tip> [--target <ref>] [--judgment <file>]` |
| `cr/v5` PASS | only when the diff changes code paths `src/ xtask/ tests/ ui/src/ ui/e2e/` (`is_code` in `xtask/src/notes.rs`); bound by `diff_sha256` over those paths | the code-review skill, on that tip (CLAUDE.md still says `cr/v2`; the gate writes `cr/v5`) |
| `design/v1` | diff carries a closed design run / component design, or touches a non-test `.tsx` under `ui/src/` | the design-review skill |
| `mr/v1` | always, signed last, attests the result report's bytes | owner via `mr confirm` |
| `dod/v1` | receipt written by the merge hook on green | hook |

Docs/infra-only diffs (no code path, no design artifact) are exempt from test/doc/cr, but every WU and epic still needs its `mr/v1`, and an epic tip still runs the tracked-doc layout check (nothing tracked under `docs/work/`).

Expected pre-sign state: `cargo xtask verify-dod wu HEAD --target epic/<slug>` red **only** on `8·manual-review`. Detect a signature: `git notes --ref=mr show <full sha>`.

Rules that bit:
- Any tree change after a binding orphans it; the notes and cr must sit on the exact tip the owner signs. Never amend/rebase a gated branch.
- A docs-only commit does not change the cr hash, but the gate looks for a cr note **on the new tip**: run the code-review skill there (cheap, diff unchanged). Never copy.
- Messages with line breaks: `-m $'subject\n\nbody'`, apostrophes `\'`. A plain `"...\n..."` landed a literal `\n` in commit c9421406.
- Rule-bearing docs = T0 `CLAUDE.md`/`AGENTS.md` + T1 flat `docs/*.html` (`is_rule_bearing`) → need `mr commit` (Reviewed-By trailer). T2/T3 and `.claude/skills/` need none. Epic plans can add owner rulings (bg-service Q14 put architecture + roadmap edits under the owner's sign-off).

## 4. Sign-off sequences (stage exactly these, cd-prefixed)

Full templates: `references/commands.md`.

**WU, no rule-bearing doc:** worker gates → router verifies → owner `mr confirm wu <tip> --as vuluu --target epic/<slug> --report docs/work/<result>.html` → owner `git switch epic/<slug> && git merge --no-ff wu/<epic>/<slug>` → router verifies the merge → fresh worker for the next WU.

**WU with a rule-bearing doc (DEC, T1, CLAUDE.md):**
1. Worker leaves only that doc staged; owner `mr commit -m $'...' --as vuluu` → new tip D.
2. Fresh worker "step between" on D: `notes write`, cr via the code-review skill on D, `preflight`, `verify-dod` (only 8 red).
3. Owner `mr confirm` on D, then merge.

**Epic close** (E-CI pattern): merge main into the epic if it moved (merge commit) → roadmap/T2 distill commits → archive commit moving the epic plan, WU plans/results and design runs to git-ignored `docs/.archive/` (DEC-3) → epic result report in `docs/.archive/` (written last, never tracked) → epic `notes write` (judgment file) → `preflight epic` → epic-scope cr vs main (only if the epic changes code paths) → `verify-dod epic` → owner `mr confirm epic <tip> --as vuluu --target main --report docs/.archive/<date>-<slug>-epic-result.html` → owner `cd <main checkout> && git merge --no-ff epic/<slug>` → owner `git push origin main`.
- Merge two epics into main one at a time; the second merges the new main in before its final gates.
- Copy every `docs/.archive/*` file to the main checkout's `docs/.archive/` (they exist only in the worktree).
- A close may land as standalone commits on the epic (E-PKG2.W6 did) when a WU confirm cannot precede the archive commit; say so to the owner.
- Smallest epics (E-RCFIX, E-PORTFLAKE, E-ROUTER) collapse to standalone commits on the epic, no WU: one `mr confirm epic`, one merge.

## 5. Independent review: paths outside the cr gate

The gate never reviews `ui/src-tauri/`, `login-service/`, `.github/`, `Cargo.lock`, `ui/package.json`, tailwind config, or docs. Every WU or epic touching them gets a router-run, read-only Opus review **before** the owner signs (spawn with `model: opus`). Checklist in `references/review-checklists.md`; the hunts that found real defects:

- **Quit/exit paths vs DEC-64**: any `app.exit`/`quit_app` path that skips the system-proxy check (found: failed window reopen exited; stale quit flag quit later).
- **`unsafe` confinement (DEC-84)**: only `login-service/`'s one module; soundness of each ObjC call; macOS 13 API on macOS 11/12 (found: SMAppService lookup would abort).
- **Signing key (DEC-82, DEC-86 rule 4)**: key only in `--sign-only`; no pnpm/install/third-party tool beside it (found: AppImage downloads beside the key; pnpm 11 verify-deps-before-run).
- **Release workflow traps**: tag/ref injection in `run:`, actions pinned by SHA, no caches, `pipefail` + early-exit readers, artifact flattening, version scheme per bundler (found: WiX rejects `rc.1`; rpm rejects any `-`).
- **Test isolation**: e2e/package tests never touch the real `~/Library/Application Support/dev.trawler.app`, login items, system proxy, or a running Trawler (found: a spec deleting the real `keep-capturing.json`; single-instance socket shared with the owner's app).
- **Back-merges**: compare both parents on every conflicted file (found: a merge silently dropped a guard).

Triage per `router-agent` §8. Report to the owner what had no independent review.

## 6. Docs: tiers and edit rights

| Tier | Where | Edit rule |
|---|---|---|
| T0 | `CLAUDE.md` | only on the owner's explicit request or an explicit plan ruling (E-PKG2 Q13: "May W6 edit CLAUDE.md's Commands line? Yes, via mr commit"); owner declined an `unsafe_code` qualifier |
| T1 | flat `docs/*.html` | `mr commit` |
| T2 | `docs/use-cases/`, `docs/decisions/`, `docs/design-rules/` | worker, inside its WU |
| T3 | `docs/work/` → archived to `docs/.archive/` at epic close | worker |

All docs are interactive HTML (htmldoc skill). Owner comments on a doc → a worker addresses them (snapshot, resolve, `?diff=latest` link for the owner). DEC numbers: check every branch before minting (DEC-77 clashed between two epics; router assigned DEC-81).

## 7. Packaging and release

- `release.yml` runs on `v*` tags only; job 1 requires tag == workspace version (`[workspace.package] version`). It builds **the tagged commit**: tagging the epic tip works; merging to main first is not required (the owner prefers: tag the epic, merge to main only after a green run). Check what the owner already did before staging.
- Versions: numeric pre-release only (`0.1.0-1`, never `0.1.0-rc.1`: WiX, DEC-86 rule 7). rpm rejects any `-` → a pre-release ships no rpm (16 updater entries; finals 18): E-RELFIX.W1, on `epic/release-fixes` until it merges.
- Each retry = version bump + new tag (`v0.1.0-1`, `-2`, `-3` exist). Never move or delete a pushed tag.
- `fail-fast: false` on the release matrix: added by E-RELFIX.W1 on `epic/release-fixes`; `main` at 1da1cf41 still has `fail-fast: true`. Check the file on the branch you tag.
- Releases are drafts; the owner publishes. Five targets; first real runs are the only proof of Windows/Linux/ARM paths.
- Updater key: `~/.tauri/trawler-updater.key` is the owner's alone. Router may read only `.pub`; check secrets by name only (`gh secret list`: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). No agent generates, reads or uploads the key (Q22 b).
- Run `actionlint` on every workflow the tip changes; workers could not execute a downloaded binary (classifier), so use the owner-installed `/opt/homebrew/bin/actionlint`.
- Hotfixes: an epic (e.g. `epic/release-fixes`, E-RELFIX) with one worktree per parallel WU (`wu/<epic-slug>/<slug>`); rc tags on the epic tip; main gets the epic once a tag run is fully green (owner ruling).

## 8. CI facts

- `ci.yml`: push to main + `workflow_dispatch`; `macos-latest`, `ubuntu-22.04` (+ xvfb), `windows-latest`; concurrency `cancel-in-progress` (explains a "cancelled" macOS job). A signal, not a gate (DEC-83). A dispatch on a branch needs the owner to push it.
- Toolchain pinned 1.99.0 (`rust-toolchain.toml` + `toolchain:` in workflows, DEC-85).
- Windows layers found one per run (DEC-85): wdio plugin 1.5.1 · `pnpm.cmd` · `cfg(unix)` dead code/imports under `-D warnings` · Common Controls v6 manifest for test executables · local origin `http://tauri.localhost` · `eol=lf`. Brief fix workers to predict the next layer (cross-clippy `xtask` for the Windows target, simulated cfg) before asking for a run.
- Windows job logs appear only after the whole run completes.
- One e2e at a time (port 4445). Check `lsof -iTCP:4445 -sTCP:LISTEN`; an orphan `trawler-ui` (ppid 1) blocks every worktree. `onboardingMatrix.test.tsx` is slow (>10 min), not hung.

## 9. The owner's real machine: never touch

- `~/Library/Application Support/dev.trawler.app/` (keep-capturing.json, CA, settings), `~/Library/LaunchAgents/dev.trawler.app.plist`, Login Items/BTM, the system proxy, a running Trawler.
- Try-recipes that need a state change: back up (`mv ... keep-capturing.json ~/kc.bak`, not `/tmp`, which a reboot wipes) and restore; say so. (A router recipe once wrote `{"keepCapturing":true}` into the real file; it stayed on and later broke an e2e quit helper.)
- Agents may not register login items (classifier: "Unauthorized Persistence"); the owner hand-tries with the packaged app.
- macOS BTM facts: SMAppService needs a bundled app (`cargo xtask package`; `cargo run` reports "unavailable") and macOS 13+. `unregister` flips the agent record enabled (0xb) → disabled (0xa) and unloads it from launchd, but the System Settings row stays (Apple DTS: it is the "Allow in the Background" list). Read-only checks: `sfltool dumpbtm | grep -A6 'Identifier: 8.dev.trawler.app'`, `launchctl print gui/$(id -u)/dev.trawler.app`, `pgrep -lf 'Trawler.app/Contents/MacOS/trawler-ui'`. Never `sfltool resetbtm` (resets every app).

## 10. Workers on this project

- Spawn template, retire, watchers: `references/commands.md`. Model: `--model claude-opus-5-5` always (owner: "they should use opus 5.5, not sonnet"). Permission mode `auto`.
- Names: `trawler-<area>-<phase>` (`trawler-pkg2-w4fix2`, `trawler-bg-close`, `trawler-relfix-w2`).
- Briefs: `<router dir>/briefs/<area>-<phase>-brief.md` (a `briefs/` folder in the router's session scratchpad, mirrored somewhere durable); skeleton in `references/brief-skeleton.md`. Standing briefs per stream (e.g. `inspect-v2-w5-brief.md`) are referenced by later briefs; keep them current.
- Brief every worker that its report goes to the router's **current** name.

## 11. Telegram

Plugin `telegram@claude-plugins-official`; its bot token lives in `~/.claude/channels/telegram/.env` and its allowlist in `~/.claude/channels/telegram/access.json`: never read, print or edit either (access changes are the owner's, via `/telegram:access` in their terminal). The router session is outbound-only unless launched with `--channels plugin:telegram@claude-plugins-official` (plugin README; not in `claude --help`); the owner answers in the terminal. Send with the reply tool in the same turn as each ask; retry once on "Network request failed".

## 12. Known failure modes

`references/failure-modes.md`.
