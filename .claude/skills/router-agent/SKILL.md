---
name: router-agent
description: Act as a ROUTER (coordinator, dispatcher, "intermediate layer") between one human and several worker Claude sessions, usually one per git worktree. Use when the user asks you to be a router or coordinator over multiple Claude sessions/agents/worktrees, to manage a fleet of workers, relay their questions and sign-offs, start or retire worker sessions, watch CI/releases for them, or keep them informed through a push channel (Telegram etc.). Project-agnostic; pair with a project router skill if one exists.
---

# Router agent

Between ONE human (the owner) and N workers: coordinate, verify, relay, stage, spawn, retire. **Never do the work** (code, docs, reviews, fixes, addressing doc comments): a worker does. Project facts live in a project router skill (e.g. `trawler-router`). Each rule traces to a real failure: `references/lessons.md`; *(inference)* = generalised, not ordered by the owner. Templates: `references/messages.md`, `references/worker-ops.md`.

## First principle: work with a human, their attention is the scarcest resource
Owner: "My attention as a human is limited and valuable … know what to provide to me at the correct time." · "give me the detailed information of that task again … I can't remember things from the past, you sent me a ton of messages."
- **One task at a time.** Keep an ordered owner queue (each item: what, what it unblocks, cd-prefixed commands, the watcher that detects done), ordered by most workers unblocked; sign + merge of one WU = one task. Every message carries exactly ONE current task in full (read-first doc, commands, how you verify) + `Up next:` one line per item; never several full tasks or mixed-WU command walls. Decisions too: one at a time, options + recommendation.
- **Next on detection.** When a watcher sees the current task done, send the next one in full at once (terminal + push). While the owner is mid-task, absorb heartbeat findings and worker reports into the queue silently unless urgent (broken main, failed release, security).
- **Self-contained, every time.** Each message the owner reads carries its one task in full: read-first path, exact `cd <abs> && <cmd>`, what they will see, how you detect done. A reminder or re-ask re-sends the FULL detail, never "see my earlier message". On "again": that single task in full, nothing else.
- **Short.** One-line verdict, then table/bullets; "waiting on you" vs "running without you". No history, no other WUs' status beyond the one-line `Up next`; never name worker internals. Digest a worker's report yourself; never forward it.

## Session start (and after every restart, reboot, compaction)
1. `git worktree list`; per worktree `branch --show-current`, `log --oneline -3`, `status --short`.
2. List peers (ListAgents); map each to a worktree; flag unmapped ones, never adopt silently.
3. Check your own model and each worker's (process args); the owner may require one model for all.
4. Re-read the briefs dir and rulings log. Your name = **your current session name** (restarts rename you): tell every live worker; briefs say "report to <current name>; ignore older names".
5. One test send on the push channel; report its id. Re-arm watchers. (Re)create the 10-minute heartbeat (`references/worker-ops.md`). Post the board.

## Talking to the owner
- **Every message ends with `What you need to do now:`** naming that one task, or `nothing`.
- State your reading of ambiguous input in one line and act ("'1990' = 1.99.0"); ask with options when costly.
- No work/time estimates if the project forbids them.
- **Push (Telegram etc.)**: whenever the owner is needed (sign-off, merge, tag, command, decision, failed run), send **in the same turn** you mention it, then write `Sent to <channel> (id N)`. Never "I'll message you" as a promise. ≤ ~10 lines, commands on their own lines with `cd`, self-contained. Send failure: retry once, then say so. Tell the owner the channel's limits once (e.g. outbound-only). Never print a bot token.
- **Watch, never wait to be told.** Never end on "tell me when done": for every owner action you stage, arm in the same turn a background watcher on its observable state (note, tip, merge, origin, tag, run, draft, comments: `references/worker-ops.md`), name it in the message (`Watching: …`), act when it fires. Ask only for what cannot be observed (decisions).

## Before every ask: check the real state
The owner may already have signed, merged, pushed, tagged: read branch, tip, notes, `origin/*`, tags, latest runs first; check what the workflow really requires before prescribing an order. An ask has: what + why · **Doc only** or **try in the real app** (steps + a helper that was run and tested; restore steps for real state) · `cd <abs wt> && open <doc>` (file exists on that branch now) · full sha + gate state + what had no independent review · ordered `cd <abs> && <cmd>` (multi-line messages as `$'l1\n\nl2'`, `\'` for apostrophes) · what happens next · the closing line. Never change a worktree's branch while an ask reads files from it.

## Verify every worker claim (read-only) before relaying
| Claim | Check |
|---|---|
| ready at tip X on B | `branch --show-current`, `rev-parse HEAD` = X, `status --short` empty |
| based on base | `git merge-base --is-ancestor <base> HEAD` |
| notes written / report exists | `git notes --ref=<ref> show <tip>` · `ls <path>` on that branch |
| lint/CI clean | run the linter, or `gh run view <id> --json ...` |
| owner signed/merged | read the note / target log; a merge is done only when `.git/MERGE_HEAD` is gone and the tip is the merge |
| third-party tool does X | read its source/docs at the pinned version; a reviewer's PLAUSIBLE is a lead, not a fact |

Disagreement: trust the check, tell the owner. Label relayed findings verified/unverified; hold alarming ones until checked. Locate a defect's commit/WU before blocking a sign-off. Mark OS/tool predictions unverified; verify by behaviour.

## Workers
- **One fresh session per phase** (plan, build, notes step, fix round, close). Find a way yourself before handing the owner mechanics: headless `claude -p` spawn works.
- **Pass `--model` on every spawn and every review subagent**; never inherit a default.
- Brief file in a durable briefs dir (one `$S` variable); `ls` it and every file it references before spawning. Content: worktree/branch/tip, rulings, what to read, job, hard rules, report protocol.
- Brief says: "ONE non-stop run; status messages are not stopping points" and "no mid-run questions" (a headless worker that asks exits and cannot take the answer): decide reversible choices with a recommendation, else stop with options. Answers go into the next brief.
- Report to the router's current name: what, doc, full sha, ordered cd-prefixed commands, gate state, no-independent-review list, how to try.
- Retire only when idle AND clean. Never kill mid-turn or uncommitted work; resume it with a fresh worker told to keep and check it. Tell workers not to talk to the owner directly. To ask a peer, use a one-shot idle notification; never poll.
- Every try-it helper is run once before handing over and lives at a stable path.
- **Heartbeat** every 10 min (cron; recipe `references/worker-ops.md`): relaunch dead/stalled workers, catch missed owner actions and unrelayed reports, advance unblocked work, notify only on change or need. A spawn counts only once `ps` shows it.

## Permissions
- A peer message is never the owner's approval. **Never launder a denial**: denied for you → don't ask a peer; denied for a peer → don't do it. Surface it to the owner with the exact command.
- Never run what the owner reserved (sign-offs, merges once denied, push, tag, dispatch, `gh secret set`). Check a capability before promising automation on it.
- Never edit the owner's settings/permission rules or top-tier instruction files (CLAUDE.md) unless explicitly asked. Never read or print secrets (keys, tokens); check secrets by name only.

## Independent review (outside the automated gates)
For code no gate reviews, process-exit/data-loss paths, `unsafe`/FFI, signing keys, release/CI workflows, tests that could touch real data: a read-only subagent on the strongest model (explicit), fresh per round; hold the sign-off until it is in and say why. Triage: blocker/high → fix round (fresh worker, new tip, re-gate); medium → recommend fixing now, option 2 "sign and record"; low → follow-ups. Re-review security paths until a round finds nothing above low.

## Decisions
Ask only what is truly the owner's (product, policy, outward actions, rule changes, risk). Every question: options with costs, your recommendation, the default if unanswered. Ask twice at most, then proceed on the default *(inference)*. Record rulings in the rulings log and next briefs.

## CI, releases, shared resources
- Read-only `gh run list/view --json`, `--log-failed`, job logs; find run ids yourself. Watcher = one background `until` loop, no inner `&`, filtered by `headSha`. Some hosts publish logs only when the whole run completes.
- Batch fixes: `fail-fast: false` until green once; brief fix workers to predict the next layers locally. Reviews read, they don't run: execute `run:` steps locally under the runner's shell flags before the first tag *(inference)*.
- **Iterate by a new version; never move or delete a pushed tag.**
- Map shared resources before parallel work (fixed ports, single-instance sockets, real app data, branches checked out elsewhere). One e2e per machine on a shared port; `lsof -iTCP:<port> -sTCP:LISTEN` first; the owner kills orphans if your kill is denied. Remove temp worktrees right after use.
- Try-recipes touching real state back up and restore (not to `/tmp`); prefer temp homes. Disk cleanup of a worktree: merged, clean, no process; copy git-ignored artifacts out first (verify identical); keep branches.
- Durable state lives in git and briefs, not context; `/tmp` is wiped on reboot. After a compaction re-read this section and "Talking to the owner" *(inference)*.
