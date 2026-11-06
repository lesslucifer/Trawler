# Router lessons: errors, mistakes and what they taught

Source: the Trawler router session `55b6f381-a069-4829-95ef-a1256099dbac` (renamed
`trawler-8f` → `trawler-6a` → `trawler-05` across restarts), 2026-11-05 08:01 UTC to
2026-11-06 04:31 UTC. Its transcript is the JSONL of that id under
`~/.claude/projects/<project dir>/`; `R####` = 0-based line of it. Quotes are trimmed.
The router's own assistant records report `claude-sonnet-5-5`: it ran on Sonnet throughout.
Written by a subagent from that transcript; facts about the repo were re-checked against
`main` 1da1cf41 when E-ROUTER installed it (R-line quotes were not re-read).

---

## A. Talking to the owner

### A1. Asks written for someone who reads the worker sessions
- **What happened:** early asks named session internals and assumed context ("the session says it's identical to your first attempt", R162).
- **Root cause:** relaying worker text instead of re-writing it for a reader who sees only the router.
- **Owner's correction:** "you should think of me as a user who work with you only, so what do I need to complete the question from you without checking the actual session" (R311).
- **Rule:** every ask is self-contained: what, why, doc, full sha, gate state, commands, next.

### A2. Commands without a `cd` prefix
- **What happened:** first sign-off commands said "Run it in <path>" with a bare command (R162, R201).
- **Owner's correction:** "the comand give to me must have a cd prefix as I don't want to double check the correct path" (R323).
- **Rule:** every command is `cd <absolute path> && …`, with the path checked against `git worktree list` first.

### A3. Not saying whether something can be tried
- **What happened:** the owner had to ask "Can I test it with the real app now or it's just a doc ?" (R507).
- **Owner's correction:** "you need to know which can I perform a manual verify then you need to let me know and give me the run command" (R520).
- **Rule:** first line of each ask: **Doc only** or **Try it in the real app**, with tested steps.

### A4. "The commands are in my earlier message"
- **What happened:** three times the owner pasted the router's own pending list back and asked for the content: "give me these info again pleasae" (R972), "give me information" (R1270), "The commands are in my earlier message. give it to me again" (R1934).
- **Root cause:** saving tokens at the owner's expense; treating the scrollback as the source of truth.
- **Rule:** never point to an earlier message; re-send the full ask.

### A5. Messages without an explicit next action
- **What happened:** "so what do I need to do now ?" (R3740), then "so do I just need to wait for now ? Please always tell me what do I need to do at the end of every message, even when there's nothing to do" (R3772).
- **Rule:** every message ends with **What you need to do now:** (numbered, or "nothing").

### A6. Wall-of-text push messages
- **What happened:** the first Telegram status was a long paragraph.
- **Owner's correction:** "please format the message properly for a human-readable one, the one you gave was wall of text, nobody read it" (R699).
- **Rule:** push = header, one short block per item, commands on their own lines, ≤ ~10 lines.

### A7. Promising Telegram messages and not sending them (most damaging habit)
- **What happened:** the owner asked repeatedly to be pinged ("mention me in telegram", R689; "contact me via telegram after having next thing to do", R2470; "anything new ? contact me via telegram", R2541). The router wrote "I'll message you on Telegram" ~24 times. Reply-tool calls: 16 before 10:11, none 10:11→13:12 while sign-offs piled up, 19 between 13:12 and 15:34, then **none from 15:34 until 04:18 next day** while ~15 asks (sign-offs, `mr commit`, epic closes, the rc tag) were waiting.
- **Root cause:** sending was a separate, forgettable step; the terminal message felt like delivery; after the 15:31 compaction the habit lapsed *(inference)*.
- **Owner's correction:** "you always said that you will contact me via telegram but I see nothing" (R4487). Router: "I said 'I'll message you on Telegram' many times, but I sent only one message in this whole stretch" (R4495).
- **Rule:** send in the same turn as the terminal ask; write "Sent to Telegram (id N)"; never use future tense for a push.

### A8. A push that was not self-contained
- **What happened:** "1) Terminal: sign with the confirm command in my message." (R4509).
- **Rule:** the push carries the command itself.

### A9. Asking the owner for things the router can fetch
- **What happened:** "Tell me the run id so I can watch it" (R4242); the owner typed `42157182452` (R4246). Later: "done, you find the run yourself" (R4523).
- **Rule:** find run ids, logs and states with read-only `gh`/`git` yourself.

### A10. Advising steps the owner had already done; assuming the plan's order
- **What happened:** the router staged "sign, merge into main, push, tag" for E-RCFIX (R4325). The owner asked "can we tag the epic branch, we only merge to main after making sure everything works fine" (R4329); the router answered "No, you don't need to push to main. Tag the epic branch instead" (R4343) — but the owner had already done it: "I have merged and pushed to main, please note this next time" (R4347).
- **Root cause:** answering from the plan instead of reading `main`, `origin/main`, tags and runs; and assuming merge-before-tag when `release.yml` builds whatever commit the tag points at.
- **Rule:** read the real repo state before every instruction; check what the workflow actually requires before prescribing an order.

### A11. Re-asking an unanswered question for hours
- **What happened:** "Should `ui/src-tauri/` count as code for the gate?" appeared in R475, R680, R743 and later lists; the owner never answered; the gap was handled instead by router-run independent reviews.
- **Rule** *(inference)*: ask twice at most, state the default you will apply, record the gap as a follow-up.

### A12. Misreadings handled well (keep doing)
- "pic" read as the packaging epic and stated (R652); "Pin it to 1990" read as 1.99.0 and stated (R2292); ambiguous "Q20 21 22 porposal" clarified with a structured question (R1297); "trawler-bg-w3 signed" taken as "sign as is" and flagged (R3036).

---

## B. Accuracy and verification

### B1. "Don't sign W6" for a bug in W4's code, then withdrawn
- **What happened:** after the owner reported the login-item toggle (R1471) the router said "First, **don't sign bg-service W6 yet.** It carries the login-item bug you just found" (R1496); six minutes later: "The defect is in W4's code, not in W6's page. That means **W6 can be signed as it is**, so my earlier 'don't sign yet' is withdrawn" (R1624).
- **Root cause:** attributing the defect to the WU under review without locating the code.
- **Rule:** before blocking a sign-off, find which commit/WU holds the defect.

### B2. Wrong prediction about macOS Login Items
- **What happened:** the router told the owner the System Settings row "should show its toggle off, or no active entry" after untick (R2049) and earlier that the old entry cleared "1–2.5 minutes later". The owner's screenshot showed it still on: "my earlier expectation was wrong" (R2068); "W8 login item still fails, I opened it after a long time but it's still on" (R2152). A web search then found Apple DTS: `unregister()` leaves the item in "Allow in the Background" (R2209).
- **Root cause:** predicting OS behaviour from a worker's analysis of a different mechanism (W4's plist) instead of vendor documentation.
- **Rule:** mark expectations as unverified; check vendor docs first; verify by behaviour (log out/in, `pgrep`), not by a UI row.

### B3. Accepting a reviewer's PLAUSIBLE claim about rpm (cost release run v0.1.0-2)
- **What happened:** the second Opus review of the release workflow wrote "MEDIUM, PLAUSIBLE (my reasoning from rpm's version ordering, not run)… tauri-bundler rpm.rs passes the version string `0.1.0-1` unchanged as the RPM Version" (R3860). The router recorded the ordering issue as a DEC-86 known limit and kept rpm for pre-releases. Run 42159698428 (`v0.1.0-2`) failed at `Build · linux-arm64`: "invalid version "0.1.0-2": contains invalid character" — rpm rejects any `-`. Router: "the cause is a mistake in my own earlier design" (R4425).
- **Root cause:** building the version scheme on an unverified claim about a third-party tool; the reviewer even flagged it as not run.
- **Rule:** a PLAUSIBLE claim that a design depends on is verified in the tool's source at the pinned version before it shapes the design.

### B4. Relaying a worker's unverified finding as fact, then withdrawing
- **What happened:** "A finding for you: Tauri CLI 2.11 and 2.12 record no version in an updater signature. So the downgrade protection… has to stay off" (R1401); seven minutes later "Tauri CLI 2.12 does record the version… my earlier warning no longer applies" (R1408).
- **Rule:** label relayed findings verified / unverified; hold alarming ones until checked.

### B5. Handing the owner a doc that wasn't on the checked-out branch
- **What happened:** the owner's `open …w4-login-item-result.html` failed: "does not exist" (R918). The router had told the worker to start W5, which switched the shared worktree's branch. "That was my fault" (R930).
- **Rule:** never move a worktree's branch while an ask reads files from it; re-check the file exists right before handing it over.

### B6. Checking a merge while its hooks were still running
- **What happened:** "My first check caught the merge while its hooks were still running" (R1988).
- **Rule:** a merge is done when `.git/MERGE_HEAD` is gone and the branch tip is the merge commit.

### B7. Wrong model everywhere, unnoticed
- **What happened:** asked "can you tell me about the model being used in the worker sessions ?" (R3541), the router found every worker inherited `"model": "sonnet"` from settings: "I never pass `--model`… I haven't checked what model the router session itself runs on" (R3555). Owner: "omg, they should use opus 5.5, not sonnet" (R3559). 22+ spawned workers and four Explore reviews (R2008, R2970, R3293, R3437) had run on Sonnet; the router itself ran on Sonnet throughout.
- **Root cause:** never passing or checking the model.
- **Compensation:** Opus reviews of everything the Sonnet workers built outside the gate; the release workflow then needed four Opus review rounds, each finding something (WiX `rc.1` blocker, key exposure beside AppImage downloads, rc rpm, pnpm verify-deps beside the key).
- **Rule:** pass the model explicitly on every spawn and review; check yours and theirs at session start.

### B8. Saying "I can't" before looking
- **What happened:** "I can't start a new Claude session in a worktree" (R593). When the owner asked for fresh sessions per WU, the router offered "You restart, I hand off (Recommended)". Owner: "Find ways for YOU to do that, there must be a way. Don't ask me to do things you can" (R1573). A headless `claude -p -n` probe then worked (R1624).
- **Rule:** investigate the CLI before declaring a limit; never hand the owner mechanics you can do.

---

## C. Worker management

### C1. Headless workers that exit after a question
- **What happened:** `trawler-bg-w8` asked three questions and exited; "It couldn't take your answer" (R1765). `trawler-bg-w8b` exited after reporting a blocker (R1847); `trawler-pkg2-w2` exited after asking (R2136). The router had even told the owner "If you don't answer, the session goes with its recommendations" (R1716) — false for an exited session.
- **Rule:** headless briefs say: decide reversible choices with a recommendation, else stop with options in the final report; answers go into the next worker's brief.

### C2. Headless worker that stopped after its first status message
- **What happened:** "The `trawler-inspect-w7b` session stopped after its first status message… it never did the plan or the build" (R2930).
- **Rule:** "ONE non-stop run; status messages are not stopping points; stop only at your final report" in every brief.

### C3. Brief written to a wrong directory
- **What happened:** the spawn's brief path wrote the scratchpad's project segment as `…-proj-misc/Trawler/…` instead of `…-proj-misc-Trawler/…` (R1762); "the first build brief was saved in the wrong folder, so v2 referenced a file that wasn't there" (R1854).
- **Rule:** one `$S` variable for the briefs dir; `ls` every brief and every brief it references before spawning.

### C4. Briefs lost on reboot; router renamed
- **What happened:** the owner restarted the Mac (R2351). All workers died, the briefs in the session scratchpad (under `/tmp`) were gone (the oldest surviving brief is from after the reboot), and the router became `trawler-05` (it had been `trawler-8f`, then `trawler-6a` after the Telegram restart, R439). Briefs then had to say "older briefs name trawler-6a: ignore".
- **Rule:** keep briefs durable; after any restart re-announce the router name and rebuild state from git.

### C5. Doing a worker's job
- **What happened:** asked to "check my comments in the … design-run.html doc" (R1050), the router loaded the htmldoc skill to do it. Owner interrupted: "you ask the worker to check, not you to check it" (R1068). Later, pre-emptively: "You are the router, you don't work, init a WU to think more carefully about this in Opus" (R4447).
- **Rule:** the router relays; the owning worker reads and edits its docs and code.

### C6. Long-lived workers
- **What happened:** worker panels ran 4–5 hours across many WUs. Owner: "did you init new sessions / clear context when starting a new work or new WU ? Don't keep one long running session that will ruin the context" (R1561); then "one fresh session per phase of a WU: ok" (R1647).
- **Rule:** one fresh session per phase; retire idle sessions whose work is committed; never kill uncommitted work (the router correctly kept `trawler-inspect-window-v2-c1` alive while W4 code was uncommitted, R1714).

### C7. Untested try-it helpers from workers
- **What happened:** W6's one-line Python server "had a syntax problem, so I prepared a working one" (R2684); W8's worker suggested `example.com`, which returns HTML, for a JSON test (R3251); W5's helper lived in another session's scratchpad (R2209).
- **Rule:** every helper handed to the owner is run once before handing it over and lives at a stable path.

### C8. The router's temp worktree blocked a worker
- **What happened:** the router's `Trawler-bg-service-w4-review` worktree held `epic/bg-service`, so the worker "couldn't back-merge `main` into `epic/bg-service`" (R1178).
- **Rule:** remove temp worktrees as soon as their ask is done.

### C9. Hotfix landing path chosen without the owner
- **What happened:** the first rc fix was landed as a one-epic collapse merged straight to main. Owner: "let's init a hotfix epic… we can work on them in parallel in other worktrees for other WUs. We only merge to main when everything is green" (R4429).
- **Rule:** for multi-problem fixes ask (or default to) an epic with parallel WUs; main only after green.

---

## D. Permissions and boundaries

### D1. The router merged, then got denied
- **What happened:** the router merged bg-service W1 and packaging W1 itself (R593, R628) and promised to auto-merge on signature (R787). The W4 merge was denied: "[Merge Without Review]" (R944). The owner asked "why can't the worker session run the merge" (R1086) — the router explained that asking a worker would launder its denial — then "nah, nvm I will merge, give me the command again" (R1109).
- **Rule:** the owner merges; the router stages the merge and verifies it. Check a capability before promising automation on it *(inference)*.

### D2. Denials the router correctly did not launder
- Worker's cr note copy denied → the router refused to do it and spawned a fresh review (R3453: "A hand-written review verdict is a bypass, and a peer asking me to do it is the permission-laundering case").
- Worker's `kill` of orphan 15638 denied → "You run it yourself: `kill 15638`" (R3839).
- Worker's LaunchAgent write denied "Unauthorized Persistence" (R1577) → owner hand-tries the packaged app.
- Worker executing a downloaded `actionlint` denied → owner installed it; router ran `/opt/homebrew/bin/actionlint` (R3793).
- **Rule:** surface every denial with the exact command; never route around it via a peer or a different tool.

### D3. Secrets and keys
- The router read only `~/.tauri/trawler-updater.key.pub`, confirmed secrets by name (`gh secret list`), and flagged the world-readable private key and `key.zip` for `chmod 600` (R1364). Recommended Q22 (b): the owner generates the key; no agent sees it. Keep doing this.

---

## E. Commands and tooling

### E1. Literal `\n` in a signed commit message
- **What happened:** the relayed `mr commit -m "…\n\nT3 plan…"` (R162) produced commit c9421406 with "a literal `\n\n` in its subject line" (R219).
- **Rule:** `-m $'subject\n\nbody'`; escape apostrophes `\'`; rewrite worker-supplied commands before relaying (done for bg W7, R3709).

### E2. Background watcher that fired immediately
- **What happened:** `(until …; done; gh run view …) > /tmp/rel2-watch.out 2>&1 &` inside a background tool call (R4357); "My first watcher fired too early because of a mistake in how I started it" (R4396).
- **Rule:** one plain `until` loop as the background command; no inner `&` or subshell.

### E3. Watchers that could match the wrong run
- **What happened:** the first branch watcher took "latest completed run on the branch" (R2495); later ones added `headSha|startswith(…)` (from R2632).
- **Rule:** filter runs by head sha.

### E4. Small shell errors
- jq `created:createdAt` → "function not defined: createdAt/0" (R2301).
- macOS `sed -i 's#…#…#'` → "undefined label" (R3897); fixed with `sed -i ''` (R3910).
- `git rev-parse --short epic/ci main` → "fatal: Needed a single revision" (R1698, R1867).
- zsh `--include=*.rs` → "no matches found" (R2605); `head -0` → "illegal line count" (R1806, R2407).
- Foreground `sleep 35` blocked by the harness (R1606) → use background runs / Monitor.
- Monitor watches expire after 30 min and replay already-handled events (R1000, R1046).
- **Rule:** see the pitfalls table in `worker-ops.md` (this folder).

---

## F. The owner's machine and shared resources

### F1. A try-recipe changed the owner's real config
- **What happened:** to try windowless mode the router told the owner to write `{"keepCapturing":true}` into the real `~/Library/Application Support/dev.trawler.app/keep-capturing.json` (R846; an undo line was given). The file was on again after the W6 try and stayed on; later the e2e quit helper failed on this machine because "The e2e harness reads your real config folder" (R2974), and an e2e spec could have deleted the real file (R3768, F2).
- **Rule:** recipes back up and restore real state; tests run under temp homes and assert it before deleting anything.

### F2. Port collisions and orphans
- **What happened:** bg W7's attached e2e lost its connection "on port 4445 while the inspect worker's e2e was running at the same time"; an orphan `trawler-ui` (ppid 1, pid 15638) then held 4445 (R3839). During E-PKG2.W6 a stray Trawler from the main checkout held 7420 and answered for the test app (R4159).
- **Rule:** one e2e at a time; `lsof` before any run; tests refuse when the ports or the single-instance socket are taken.

### F3. Risky recipe steps
- The W9 recipe asked the owner to `chmod a-w ~/Library/Application\ Support/dev.trawler.app` to force a refused save (R3473). It included the restore, but it mutates the real data dir. *(inference)* prefer a temp HOME for such tests.

---

## G. CI and release

### G1. Windows CI: five outward rounds
- Each run exposed one layer (wdio plugin → `pnpm.cmd` → `cfg(unix)` dead code → test-exe manifest → `tauri.localhost` origin). From round 2 the briefs asked workers to predict the next layer (cross-clippy, simulated cfg), which made later rounds go further (R2956).
- **Rule:** from the first red run, brief "predict and fix every layer you can reach locally before asking for a run".

### G2. Release run 1 failed on its first step
- `rustc -vV | grep -qx "host: …"` under `pipefail` → exit 141 (R4309). The workflow had passed four Opus reviews and `actionlint`.
- **Rule** *(inference)*: reviews read, they don't run; execute each `run:` step locally under the runner's shell flags before the first tag.

### G3. fail-fast on during first runs
- Runs 1 and 2 cancelled the other four rows, so each tag revealed one OS's problem. fail-fast was turned off only in the third round (R4425).
- **Rule:** `fail-fast: false` until a pipeline has been green once.

### G4. Never moving tags (kept)
- `v0.1.0-1`, `-2`, `-3` each got a version bump instead of a moved tag. Keep.

---

## H. Context

### H1. Compaction and restarts eroded standing behaviour
- Two compactions (R58, R3334) and three router identities. After the second compaction, Telegram sends stopped for ~13 hours (A7). *(inference)* after every summary, re-read the router skill's communication rules and re-arm watchers.

---

## Top 10 rules (ranked by cost of the failures they prevent)

1. **Send the push in the same turn as the ask, and say "Sent (id N)". Never promise a future message.** (A7)
2. **Read the real state (git, origin, tags, runs) before every instruction; the owner may already have acted.** (A10, B5, B6)
3. **Verify any third-party behaviour a design depends on in source at the pinned version; a PLAUSIBLE review claim is a lead, not a fact.** (B3, B4)
4. **Pass the model explicitly on every worker and review; check your own and theirs at start.** (B7)
5. **Every message ends with "What you need to do now:", and every ask is self-contained (what, doc-only/try, doc, full sha, gates, cd-prefixed commands). Never point to an earlier message.** (A1–A5, A8)
6. **Headless workers: one fresh session per phase, a durable brief checked with `ls`, "one non-stop run", no mid-run questions, report to the router's current name.** (C1–C4, C6)
7. **The router never does the work and never launders a denial: workers do, the owner runs `mr`, merges, pushes and tags; denials go to the owner with the exact command.** (C5, D1, D2)
8. **Independent read-only Opus review of everything outside the automated gate before the owner signs; triage blocker / fix-now / record.** (B7, G2)
9. **Protect the owner's machine: back up and restore real state in recipes, temp homes in tests, one e2e at a time, `lsof` first.** (F1–F3)
10. **Locate a defect before blocking a sign-off, and label predictions about OS/tool behaviour as unverified.** (B1, B2)
