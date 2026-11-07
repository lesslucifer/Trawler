# Trawler failure modes

| # | Failure | Seen | Rule |
|---|---|---|---|
| 1 | Router's merge denied "Merge Without Review" | bg-service W4 | Owner merges; stage it |
| 2 | Worker's cr note copy denied | E-PKG2.W3 step between | Fresh worker runs the cr skill on the tip |
| 3 | Literal `\n` in `mr commit` | c9421406 | `-m $'…'`, `\'`; rewrite worker commands |
| 4 | Owner's report "does not exist" | bg-service W4 | No branch switch under a pending ask |
| 5 | Worker can't switch to the epic | bg back-merge | Remove router temp worktrees |
| 6 | Headless worker exits after question/status | bg W8, pkg2 W2, inspect W7b | Non-stop, no-questions brief |
| 7 | Brief not found | bg W8 build | One `$S`; `ls` every brief |
| 8 | Briefs gone after reboot | 2026-11-05 13:08 UTC | Durable briefs; rebuild from git |
| 9 | Workers/reviewers/router on Sonnet | first 22+ workers, 4 reviews | `--model claude-opus-5-5`; `model: opus` |
| 10 | Windows CI one layer per run | E-PKG2.W3, 5 rounds | Cross-clippy, simulate cfg, predict |
| 11 | macOS job "cancelled" | ci round 2 | `cancel-in-progress`, not a failure |
| 12 | Release red at host check, exit 141 | v0.1.0-1 | Capture, compare with `[ ]` (DEC-86 r8) |
| 13 | rpm "invalid version" | v0.1.0-2 | Pre-release ships no rpm (E-RELFIX.W1) |
| 14 | WiX rejects `rc.1` | review, pre-tag | `0.1.0-N` (DEC-86 r7) |
| 15 | One red row cancels the rest | v0.1.0-1, -2 | `fail-fast: false` |
| 16 | e2e collision; orphan holds 4445 | bg W7 vs inspect W9; pid 15638 | One e2e; `lsof`; owner kills if denied |
| 17 | Stray Trawler answers a test | E-PKG2.W6, port 7420 | Tests refuse held ports/socket |
| 18 | Real `keep-capturing.json` left on | router try-recipe | Backup/restore; temp HOME |
| 19 | Login item row stays in Settings | W8 | BTM allowlist; verify by behaviour |
| 20 | Agent can't register a login item | W4/W8 | "Unauthorized Persistence"; owner tries |
| 21 | Downloaded actionlint denied | E-PKG2.W4 | `/opt/homebrew/bin/actionlint` |
| 22 | DEC number clash | DEC-77 | Check all branches; router assigns |
| 23 | Back-merge dropped a guard | E-PKG2.W5 r1 | Compare both parents |
| 24 | `vitest run` looks hung | `onboardingMatrix.test.tsx` | Slow (>10 min); don't kill others' |
| 25 | Owner already merged/pushed/tagged | E-RCFIX | Read main, origin, tags, runs first |
| 26 | Router asks the owner for a run id | v0.1.0-1, -3 | `gh run list` yourself |
| 27 | Watcher fires at once | v0.1.0-2 | One until loop, no inner `&` |
| 28 | Owner had to say "done" after each action | E-RELFIX, E-RC4 | Watcher per staged action; heartbeat |
| 29 | Announced worker never started | `command -v claude` empty | Absolute path; announce after `ps` |
| 30 | Worker dead or stalled unnoticed | reboot, idle exits | Heartbeat relaunches from the brief |
