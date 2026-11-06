---
name: trawler-router
description: Run the router role on the Trawler project (Rust + Tauri MITM proxy, sibling worktrees ../Trawler-<slug>). Use when coordinating Trawler worker sessions across worktrees: staging mr commit / mr confirm / merge / push / tag commands for the owner, verifying gate notes (test/v2, doc/v1, cr/v5, design/v1, mr/v1, dod/v1), closing epics, running independent reviews of ui/src-tauri, login-service or .github workflows, spawning Opus workers with briefs, or watching CI and release runs. Load the general router-agent skill too.
---

# Trawler router

Load `router-agent` first; this adds Trawler facts. Sources: `CLAUDE.md`, `docs/workflow.html`, `docs/testing-strategy.html`, DEC-3 (`docs/decisions/workflow-gates.html`), DEC-64, 77, 81–86. Code wins: if `xtask/` or the docs disagree, trust them and fix this skill. References: `commands.md` · `brief-skeleton.md` · `review-checklists.md` · `failure-modes.md`.

## Layout
| Thing | Value |
|---|---|
| Main checkout | first row of `git worktree list` (`/Users/nguyenvanluong/Documents/proj/misc/Trawler`), `main`, merge-only (pre-commit refuses non-merge commits, DEC-6) |
| Worktrees · branches | siblings `../Trawler-<slug>`, one per epic (+ one per parallel WU) · `epic/<slug>` off main, `wu/<epic-slug>/<slug>` off its epic |
| Ids | epic `E-<CODE>`; WU `E-<CODE>.W<m>`, numeric only ("W2b" had to become W3) |
| Remote · owner id | `origin` = `git@github.com:lesslucifer/Trawler.git` (public) · `--as vuluu` (`cut -d' ' -f2 .mr-allowed`; git-ignored, copied into each new worktree) |
| Ports | 7420 proxy · 7421 control API · 4445 embedded WebDriver (e2e) |

## Who runs what
| Action | Who |
|---|---|
| Plans, code, tests, results, distillation, `cargo xtask notes write`/`preflight`/`verify-dod`, code-review skill (cr/v5), design-review skill (design/v1) | worker |
| Independent read-only review of code no gate binds; reading `gh` runs/logs, git state, `/opt/homebrew/bin/actionlint` | router (Opus subagent) |
| `cargo xtask mr commit` / `mr confirm` | **owner only**: the router stages, never runs |
| `git merge --no-ff` (router's merge was denied "Merge Without Review"), `git push`, tags, `gh workflow run`, `gh secret set`, killing a process whose kill was denied | owner |

Never hand-write or copy a gate note between tips (a cr copy was denied: it is a bypass).

## Gates (what a tip carries)
| Note | When | By |
|---|---|---|
| `test/v2`, `doc/v1` | every WU/epic tip | `cargo xtask notes write wu\|epic <tip> [--target <ref>] [--judgment <file>]` |
| `cr/v5` PASS | diff changes code paths `src/ xtask/ tests/ ui/src/ ui/e2e/` (`is_code`, `xtask/src/notes.rs`); bound by `diff_sha256` over them (CLAUDE.md still says cr/v2) | code-review skill on that tip |
| `design/v1` | closed design run / component design, or a non-test `.tsx` under `ui/src/` | design-review skill |
| `mr/v1` · `dod/v1` | always, signed last, attests the result's bytes · receipt on green | owner `mr confirm` · merge hook |

- Docs/infra-only diffs are exempt from test/doc/cr, but every WU/epic still needs `mr/v1`; an epic tip still runs the tracked-doc layout check (nothing tracked under `docs/work/`).
- Pre-sign: `cargo xtask verify-dod wu HEAD --target epic/<slug>` red **only** on `8·manual-review`. Signed? `git notes --ref=mr show <full sha>`.
- Any tree change orphans the bindings; notes + cr sit on the exact tip signed. Never amend/rebase a gated branch. A docs-only commit keeps the cr hash, but the gate wants a cr note **on the new tip**: rerun the skill (never copy).
- Rule-bearing = `CLAUDE.md`/`AGENTS.md` + flat `docs/*.html` (`is_rule_bearing`) → `mr commit` (Reviewed-By). T2/T3 and `.claude/skills/` need none.

## Sign-off sequences (stage cd-prefixed; templates in `commands.md`)
- **WU**: worker gates → router verifies → owner `mr confirm wu` → owner `git switch epic/<slug> && git merge --no-ff wu/…` → router verifies → fresh worker.
- **WU with a rule-bearing doc**: only that doc staged; owner `mr commit -m $'…'` → tip D; fresh "step between" worker on D (notes, cr skill, preflight, verify-dod); owner `mr confirm` on D, merge.
- **Epic close**: merge main in if it moved → distill commits → archive commit moving plans/results/design runs to git-ignored `docs/.archive/` (DEC-3) → epic result in `docs/.archive/` last → epic `notes write` (judgment) → `preflight epic` → epic cr vs main (code paths only) → `verify-dod epic` → owner `mr confirm epic … --target main` → owner merge in the main checkout → owner push. Two epics merge one at a time (the second merges new main first). Copy `docs/.archive/*` to the main checkout. Close may land as standalone epic commits when a WU confirm cannot precede the archive (E-PKG2.W6): tell the owner. Smallest epics (E-RCFIX, E-PORTFLAKE, E-ROUTER): standalone commits, no WU, one confirm, one merge.

## Independent review: what the cr gate never sees
`ui/src-tauri/`, `login-service/`, `.github/`, `Cargo.lock`, `ui/package.json`, tailwind config, docs. Every WU/epic touching them: router-run read-only Opus review **before** the owner signs (`review-checklists.md`); triage per `router-agent`; report what had none.

## Docs
T0 `CLAUDE.md`: only on explicit owner request or plan ruling (E-PKG2 Q13), via `mr commit` · T1 flat `docs/*.html`: `mr commit` · T2 `docs/{use-cases,decisions,design-rules}/` and T3 `docs/work/` (archived at close): worker. All HTML via htmldoc; owner comments → a worker addresses them (`?diff=latest`). Check every branch before minting a DEC number (DEC-77 clashed).

## Release and CI
- `release.yml`: `v*` tags; tag must equal `[workspace.package] version`; builds **the tagged commit** (owner prefers tagging the epic, main only after a fully green run). Drafts; the owner publishes. Five targets.
- Numeric pre-releases only (`0.1.0-1`, never `rc.1`: WiX, DEC-86 rule 7); rpm rejects any `-` → pre-releases ship no rpm (E-RELFIX.W1, `epic/release-fixes`). Each retry = bump + new tag; never move/delete a pushed tag.
- `fail-fast: false` exists only on `epic/release-fixes` (main still `true`): check the branch you tag. `actionlint` every changed workflow.
- Updater key `~/.tauri/trawler-updater.key` is the owner's alone: read only `.pub`; secrets by name (`gh secret list`: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`); no agent generates, reads or uploads it (Q22 b).
- Hotfix = an epic with one worktree per parallel WU; rc tags on the epic tip.
- `ci.yml`: push to main + `workflow_dispatch`; macOS / ubuntu-22.04 (xvfb) / Windows; `cancel-in-progress` (a "cancelled" job is not a failure); a signal, not a gate (DEC-83); a branch dispatch needs the owner's push. Toolchain 1.99.0 (DEC-85). Windows logs appear only after the whole run.
- Windows layers seen (DEC-85): wdio plugin 1.5.1 · `pnpm.cmd` · `cfg(unix)` dead code under `-D warnings` · Common Controls v6 manifest for test exes · origin `http://tauri.localhost` · `eol=lf`. Brief fix workers to cross-clippy `xtask` for Windows and simulate cfg before asking for a run.

## The owner's machine: never touch
`~/Library/Application Support/dev.trawler.app/` (keep-capturing.json, CA, settings), `~/Library/LaunchAgents/dev.trawler.app.plist`, Login Items/BTM, the system proxy, a running Trawler. Recipes back up to `~` (not `/tmp`) and restore. Agents cannot register login items ("Unauthorized Persistence"): the owner hand-tries the packaged app. BTM: SMAppService needs a bundled app (`cargo xtask package`) and macOS 13+; `unregister` disables (0xb→0xa) but the Settings row stays. Read-only: `sfltool dumpbtm | grep -A6 'Identifier: 8.dev.trawler.app'`, `launchctl print gui/$(id -u)/dev.trawler.app`, `pgrep -lf 'Trawler.app/Contents/MacOS/trawler-ui'`. Never `sfltool resetbtm`. One e2e at a time (`lsof -iTCP:4445 -sTCP:LISTEN`); `onboardingMatrix.test.tsx` takes >10 min, not hung.

## Workers and Telegram
- Always `--model claude-opus-5-5`, `--permission-mode auto`; names `trawler-<area>-<phase>`; briefs `<router dir>/briefs/<area>-<phase>-brief.md` (session scratchpad, mirrored durably), keep standing briefs current; report to the router's **current** name.
- Telegram plugin `telegram@claude-plugins-official`: never read, print or edit `~/.claude/channels/telegram/.env` (token) or `access.json` (access is the owner's, `/telegram:access`). Outbound-only unless launched with `--channels plugin:telegram@claude-plugins-official`; the owner answers in the terminal. Send in the same turn as each ask; retry once on "Network request failed".
