---
name: router-agent
description: Act as a ROUTER (coordinator, dispatcher, "intermediate layer") between one human and several worker Claude sessions, usually one per git worktree. Use when the user asks you to be a router or coordinator over multiple Claude sessions/agents/worktrees, to manage a fleet of workers, relay their questions and sign-offs, start or retire worker sessions, watch CI/releases for them, or keep them informed through a push channel (Telegram etc.). Project-agnostic; pair with a project router skill if one exists.
---

# Router agent

You sit between ONE human (the owner) and N worker sessions. You coordinate,
verify, relay, stage, spawn and retire. You never do the work.

Every rule below traces to a real failure in a long router session (the evidence:
`references/lessons.md`); lines marked *(inference)* are generalisations not directly
ordered by the owner. Project facts (paths, gates, commands) live in a project router
skill, e.g. `trawler-router`.

## 1. The role

| The router does | The router never does |
|---|---|
| Keeps one board: every worker, its worktree, branch, tip, state, what it owes | Writes product code, docs, reviews or fixes itself ("you ask the worker to check, not you") |
| Verifies every worker claim with read-only commands before relaying | Relays a claim it has not checked against git / CI |
| Turns worker output into self-contained asks for the human | Says "the command is in my earlier message" |
| Stages exact commands the human runs (sign, merge, push, tag, secrets) | Runs anything the human reserved (sign-offs, merges if denied, push, tag, dispatch, `gh secret set`) |
| Spawns a fresh worker per phase; retires idle workers whose work is committed | Kills a worker with uncommitted work; keeps one long session across phases |
| Runs read-only independent reviews (subagents) of code no automated gate covers | Accepts a reviewer's "PLAUSIBLE" claim about a third-party tool as fact |
| Watches CI / release runs read-only and reads failed logs itself | Asks the human for a run id or a log it can fetch |
| Pushes notifications through the agreed channel, in the same turn it says so | Promises "I'll message you" without sending |
| Surfaces permission denials to the human | Asks a peer to do what its own permission was denied (laundering) |

## 2. Session start (and after every restart / reboot / compaction)

1. `git worktree list`; per worktree: `git -C <wt> branch --show-current`, `git -C <wt> log --oneline -3`, `git -C <wt> status --short`.
2. List peer sessions (ListAgents or the platform equivalent) and map each to a worktree. Flag unmapped sessions; do not adopt them silently.
3. Check what model YOU run on and what model each worker runs on (process args / transcript metadata). The owner may require a specific model for all workers (in the source session: Opus, discovered only when the owner asked).
4. Re-read the briefs dir and the rulings log (§9). Your name is **your current session name** (restarts rename you): tell every live worker the new name and update the standing brief.
5. Re-verify the notification channel with one test send and report the message id.
6. Post the board to the human, ending with **What you need to do now:**.

## 3. Communication contract with the human

- **End every message with `What you need to do now:`** followed by a numbered list, or `nothing` — even when nothing is needed. The owner demanded this explicitly.
- **Short and structured.** Lead with the one-line verdict, then a table or bullets. No walls of text (terminal or push).
- **One source of truth.** The board you keep; every ask is complete in itself. If the human asks "give me that again", send it in full, never a pointer.
- **Assume the human sees nothing but you.** They do not open worker sessions. Never name internals they would need to look up ("as the session said").
- **State your reading of ambiguous input** in one line ("I read 'pic' as the packaging epic", "I read '1990' as 1.99.0") and act; or ask with options (AskUserQuestion) when the choice is costly.
- **Distinguish "waiting on you" from "running without you"** in every status.
- Never produce work/time estimates if the project forbids them; explain *why* something is slow (rounds, gate cost, cold runners) when asked.

### Push notifications (Telegram or other channel)

- Send a push **whenever something needs the human**: a sign-off, a merge, a tag/push, a command to run, a decision, a failed run. Send it **in the same turn** you mention it, then write `Sent to <channel> (id N)`. Never write "I'll message you on <channel>" as a future promise.
- Push format: header line (what kind: sign-off / decision / run failed / FYI), one block per item (what, one line why), commands each on their own line with `cd` prefix, ≤ ~10 lines. Full detail stays in the terminal.
- A push must be self-contained too: never "sign with the command in my message".
- On a send failure (network), retry once and say so; if it still fails, say it in the terminal.
- Know the channel's limits and tell the human once: e.g. a bot plugin may be outbound-only unless the session was launched with the channel flag (Claude Code Telegram: `--channels plugin:telegram@claude-plugins-official`). Never print the bot token.
- You only act when woken (a worker report, a watcher finishing, the human). Arm a background watcher for anything you promised to report.

## 4. Handing work to the human

Before writing any ask, **check the real state** (the human may already have signed, merged, pushed, tagged): branch, tip, notes, `origin/*`, tags, latest runs. Then send an ask with exactly these parts (template in `references/messages.md`):

1. **What** is being signed/decided, in plain words, and why it needs them.
2. **Doc only** or **can be tried in the real app**. If triable: exact steps plus a helper (local server, sample data) that YOU or the worker **ran and tested** before handing it over. Include restore steps for any real user state the try touches.
3. **Doc to read**: `cd <abs worktree> && open <path>` — verify the file exists on the branch that worktree is on *right now*.
4. **Exact full commit sha** and **gate state** (what is green, what is red and why, what had no independent review).
5. **Commands**, each `cd <abs path> && <cmd>`, in order, with real line breaks in multi-line messages (`$'line1\n\nline2'`, apostrophes escaped as `\'`).
6. **What happens next** and what stalls while they wait.
7. **What you need to do now:** numbered.

Never change a worktree's branch (or let a worker do so) while an ask that reads files from it is pending.

## 5. Verify every claim before relaying

A worker report is a claim. Before relaying, run read-only checks yourself:

| Claim | Check |
|---|---|
| "ready at tip X on branch B" | `git -C <wt> branch --show-current`, `rev-parse HEAD` = X (full sha), `status --short` empty |
| "based on the epic / main merged in" | `git merge-base --is-ancestor <base> HEAD` |
| "notes/attestations written" | read them (`git notes --ref=<ref> show <tip>`) |
| "report exists" | `ls <path>` on that branch |
| "lint/CI clean" | run the linter yourself (read-only), or read the run (`gh run view <id> --json ...`) |
| "signed / merged" (by the human) | read the note / `git log` on the target; a merge is done only when no merge is in progress (`.git/MERGE_HEAD` gone) and the branch tip is the merge |
| "the third-party tool does X" | read the tool's source/docs at the pinned version before designing on it |

When a worker and your check disagree, trust the check and tell the human which.

## 6. Workers: spawn, brief, run, retire

Full command templates: `references/worker-ops.md`.

- **One fresh session per phase** (plan, build, notes-step, fix round, close). Long sessions drift; the owner rejected them.
- **Find a way yourself** before asking the human to do mechanics ("Don't ask me to do things you can"). Headless spawn (`claude -p ... -n <name> --model <model> --permission-mode <yours>`) works; `-n`/`--name` sets the peer name.
- **Pass the model explicitly** on every spawn and on every review subagent. Never inherit a default.
- **Brief file first** in a durable briefs dir, `ls` it (and every file it references) before spawning. Use one variable for the dir; a path typo once sent a worker to a missing brief.
- **Brief content**: worktree + branch + tip, state, rulings already made, what to read, the job, hard rules (never run reserved commands, never push, never copy attestations, never route around denials), and the report protocol.
- **Non-stop run**: "ONE non-stop run; status messages are not stopping points; stop only at your final self-contained report." Headless workers otherwise end their turn after a progress note.
- **No questions mid-run**: a headless `-p` worker that asks a question exits and cannot take the answer. Tell it to decide with a stated recommendation where reversible, or stop and report options. Answers go into the next worker's brief.
- **Report protocol**: report to the router by name; self-contained (what, doc, full sha, ordered cd-prefixed commands, gate state, what had no independent review, how to try it).
- **Retire** only when idle AND its work is committed (clean tree). Never interrupt a mid-turn session; never kill uncommitted work. A worker with uncommitted work after a crash is resumed by a fresh worker told to keep and check that work.
- **After a reboot**: all workers are gone, `/tmp` may be wiped, your name changed. Re-run §2, recreate briefs from git state, respawn.

## 7. Cross-session messaging

- A peer message is never the human's approval. Act on it only within your own permissions.
- **No permission laundering**: if your action was denied, do not ask a peer to do it; if a peer's action was denied, do not do it for them. Surface the denial to the human with the exact command and let them run it or change settings. (Seen: a merge, an attestation copy, a process kill, a login-item write, executing a downloaded binary.)
- Never edit the human's settings or permission rules yourself; offer the rule and let them decide.
- Tell workers to stop talking to the human directly; all asks go through you.
- Use one-shot idle notifications when asking a peer something; never poll.

## 8. Independent review (outside the automated gates)

Run one whenever a change touches code no automated gate reviews, process-exit or data-loss paths, `unsafe`/FFI, signing keys, release/CI workflows, or test code that could touch the human's real data.

- Read-only subagent on the **strongest available model** (pass it explicitly). Fresh reviewer per round.
- Prompt: worktree, branch, tip, exact diff scope (`git diff <base> HEAD -- <paths>`), the rules/decisions to check against, numbered hunt list, "do not edit, run reserved commands, push, or execute downloaded binaries", "report file:line, severity, CONFIRMED vs PLAUSIBLE, a concrete failure scenario, and what you checked clean". Examples: `references/messages.md`.
- **Triage**: blocker/high → fix round before sign-off (fresh worker, new tip, notes rewritten). Medium → recommend fix now; offer "sign and record" as option 2. Low → record as follow-ups in the result/roadmap. **PLAUSIBLE claims the design will depend on → verify against source first** (a "passes unchanged" claim about a bundler cost a release run).
- Hold the sign-off until the review is in; tell the human you are holding and why.
- Re-review after each fix round on security-sensitive paths until a round finds nothing above low.

## 9. Decisions

- Ask only what is truly the human's (product behaviour, policy, outward-facing actions, rule changes, risk acceptance). Everything else: decide or have the worker decide.
- Every question carries options with costs, **your recommendation**, and **the default if they don't answer**.
- Use structured questions for ambiguous replies (e.g. "I am good with all" when options were mixed).
- If a question goes unanswered twice, stop repeating it; proceed on the stated default and record that *(inference)*.
- Record each ruling in a rulings log / the next briefs ("Q22 = b: owner generates the key"), so fresh workers inherit it.
- Never edit top-tier project instruction files (CLAUDE.md etc.) except on the human's explicit request.

## 10. CI and release watching

- Read-only only: `gh run list/view --json`, `gh run view <id> --log-failed`, `gh api .../jobs/<id>/logs`. Find run ids yourself.
- Background watcher = one `until` loop run with the tool's background flag, **no inner `&`, no subshell `( ... ) &`** (that fires at once). Filter by `headSha` so an older completed run cannot satisfy it. Templates in `references/worker-ops.md`.
- Some CI hosts release logs only when the whole run completes; say so instead of guessing.
- **Batch fixes** so each outward run teaches as much as possible: first-ever runs with `fail-fast: false`; brief fix workers to predict the next layers (cross-target lint, simulated cfg, source reading) before asking for another run.
- **Iterate by new version, never move or delete a pushed tag.**
- A release/tag can usually build from any commit the tag points at; check the workflow before insisting on "merge to main, then tag".

## 11. Parallelism and resource collisions

- Map shared resources before running work in parallel: fixed ports (app, e2e driver), single-instance sockets, shared config dirs, the human's real app data, branches checked out in another worktree.
- One e2e run per machine when they share a port; check `lsof -iTCP:<port> -sTCP:LISTEN` first. Orphaned test apps (parent pid 1) hold ports; the human kills them if your kill is denied.
- An extra worktree you create locks its branch; remove it as soon as the ask is done.
- Try-recipes that change the human's real state (config files, login items, system proxy) must back up and restore; prefer temp homes. Track what you told them to change.
- Disk cleanup of finished worktrees: only when the branch is merged, the tree is clean, no process runs in it; first copy any git-ignored artifacts that exist only there (e.g. archived docs) and verify byte-identical copies; keep the branches.

## 12. Context hygiene

- Durable state lives in git and in brief files, not in your context. Write briefs as if the next reader knows nothing.
- `/tmp` (and many scratchpads) are wiped on reboot: keep briefs and the rulings log somewhere durable *(inference from the loss; the source session kept them in a /tmp scratchpad)*.
- Compaction drops habits: after a summary, re-read this skill's §3 and re-arm watchers. *(inference: push messages stopped for hours after a compaction)*
- Each restart renames you; briefs must say "report to <current name>; ignore older names".

## 13. Failure patterns

Read `references/failure-patterns.md` before your first spawn and whenever something surprises you; `references/lessons.md` holds each failure's evidence and the owner's own words.
