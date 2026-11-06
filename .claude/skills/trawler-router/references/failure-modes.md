# Trawler: known recurring failure modes

| # | Failure | Seen | Fix / rule |
|---|---|---|---|
| 1 | Router's merge denied: "Merge Without Review" | bg-service W4 | Owner merges; stage `cd … && git switch epic/… && git merge --no-ff …` |
| 2 | Worker's cr note copy onto a new tip denied | E-PKG2.W3 step between | Fresh worker runs the code-review skill on that tip; never copy, never ask the router to |
| 3 | Literal `\n` in an `mr commit` message | E-INSPWIN2 epic plan c9421406 | `-m $'…\n\n…'`, `\'` for apostrophes; rewrite worker-supplied commands |
| 4 | Owner opens a report: "file does not exist" | bg-service W4 | Worker switched the worktree to W5's branch; don't move a branch under a pending ask |
| 5 | Worker can't switch to the epic | bg-service back-merge | Router's temp review worktree held `epic/bg-service`; remove temp worktrees |
| 6 | Headless worker exits after a question / status note | bg W8, pkg2 W2, inspect W7b | Non-stop + no-questions brief; new worker with the rulings |
| 7 | Brief not found | bg W8 build | Path typo `-misc/Trawler/` for `-misc-Trawler`; one `$S`, `ls` every brief |
| 8 | Briefs gone after reboot | 2026-11-05 13:08 UTC | /tmp scratchpad wiped; recreate from git; keep briefs durable |
| 9 | Workers and reviewers on Sonnet | the first 22+ spawned workers, 4 Explore reviews, the router itself | `--model claude-opus-5-5`; `model: opus` on reviews |
| 10 | Windows CI red one layer per run | E-PKG2.W3, 5 rounds | Predict: cross-clippy xtask for Windows, simulate cfg(unix) off, read the next tests' assumptions |
| 11 | macOS job "cancelled" | ci round 2 | `cancel-in-progress` concurrency, not a failure |
| 12 | Release run red at host check (exit 141) | v0.1.0-1 | `grep -q` under pipefail; capture with `sed`, compare with `[ ]` (DEC-86 rule 8) |
| 13 | Release run red at rpm bundle "invalid version" | v0.1.0-2 | rpm rejects `-`; pre-release ships no rpm (E-RELFIX.W1) |
| 14 | WiX rejects `rc.1` | caught by review before tagging | Numeric pre-release `0.1.0-N` (DEC-86 rule 7) |
| 15 | One red row cancels all others | v0.1.0-1, v0.1.0-2 | `fail-fast: false` |
| 16 | e2e collision / port 4445 held | bg W7 vs inspect W9; orphan pid 15638 | One e2e at a time; `lsof`; owner kills orphans when the worker's kill is denied |
| 17 | Window test answered by a stray Trawler | E-PKG2.W6 (port 7420 held by main checkout app) | Test refuses when ports/socket held; quit other Trawlers |
| 18 | Real `keep-capturing.json` left on | router's windowless try-recipe | Backup/restore in every recipe; e2e under temp HOME |
| 19 | Login item row stays in System Settings | W8 | macOS behaviour (BTM allowlist); verify by behaviour (log out/in, `pgrep`) not by the row |
| 20 | Agent can't register a login item | W4/W8 | Classifier "Unauthorized Persistence"; owner hand-tries the packaged app |
| 21 | Worker can't run downloaded actionlint | E-PKG2.W4 | Owner installs via brew; router runs `/opt/homebrew/bin/actionlint` |
| 22 | DEC number clash across epics | DEC-77 | Check all branches before minting; router assigns |
| 23 | Back-merge silently dropped a guard | E-PKG2.W5 round 1 | Review both parents of conflicted files |
| 24 | `vitest run` looks hung | `onboardingMatrix.test.tsx` | Slow, not hung (>10 min); don't kill other workers' vitest |
| 25 | Owner already merged/pushed/tagged | E-RCFIX | Read main, origin, tags, runs before every instruction |
| 26 | Router asks owner for run id | v0.1.0-1, v0.1.0-3 | `gh run list --workflow release.yml -L 3` yourself |
| 27 | Watcher fires at once | release v0.1.0-2 | No inner `&`; one background until-loop |
