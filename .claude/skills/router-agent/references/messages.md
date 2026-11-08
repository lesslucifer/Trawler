# Ask-comment and ping templates

The owner reads tasks, not chat (SKILL.md "Tracker loop"). An ask is one comment, written whole, then the owner assigned and the needs-owner label added in the same call (the project's tracker protocol has the command, e.g. `vikunja-protocol.md`). Write each body to a file under `$S` and pass it on stdin.

**Sign-off ask** (on the WU's / epic's task)
```
**<Item> is ready for you to sign.** Checked: tip `<full sha>` on `<branch>`, tree clean, <gates> on the tip; only your signature is red.
**What you're signing:** <2–5 bullets in user terms>
**Doc only** | **Try it in the real app** (tested helper; restore steps)
**Gate state:** <green> · **Not independently reviewed:** <paths | none>
**Read first:** `cd <abs worktree> && open <doc>`
**Run in order:**
<```sh fence: 1. cd <abs> && <sign command> · 2. cd <abs> && <merge command>>
**Watching:** <probe that detects done>; no label change needed, the router closes this ask when it fires.
```

**Decision ask**
```
**<Item> needs <N> decision(s).** <context, one line>
**Q1 <question>**
- A — <means> · cost <…> (**my pick**)
- B — <means> · cost <…>
Default if you just remove the label: A.
Answer in a comment (e.g. "Q1 B"), then remove the `needs-owner` label. Marking the task done closes it unanswered and nothing runs.
```

**Re-ask**: never; the ping is the reminder. A changed ask is a new comment written whole (read-first, commands, what they will see, `Watching`, default); no history, no other tasks.

**Status comment** (no ask, nobody pinged): `<event>: <one line>` — `worker trawler-x-build spawned` · `report: gates green at <sha8>` · `seen <sha8>` · `merged <sha8> into epic/<slug>`.

**Ping** (the only push): `Trawler: #<id> <title> needs you` · `Trawler: <branch>[, <branch>…] had no task, now tracked` · `Trawler: Vikunja down, relaunch failed, router keeps retrying`. One plain line, no command, no status, no secret.

**Independent review prompt** (read-only subagent, strongest model, explicit)
```
Independent READ-ONLY review (very thorough) of <what>, which the automated gate does not bind.
Worktree <abs>, branch <b>, tip <sha>; base <sha>. Do not edit, run <reserved commands>, push, tag, or execute
downloaded binaries; git/grep/read and read-only gh api only. Scope: `git diff <base> HEAD -- <paths>` + <files>.
Rules: <decision docs>. Already covered by earlier rounds: <list>.
Hunt: (1) exit paths skipping a safety check (2) unsafe/FFI soundness, confinement (3) secret exposure: only in
the step that needs it, never logged, unreachable by third-party code (4) workflow traps: ref injection in run:,
unpinned actions, caches, pipefail, artifact naming (5) tests touching real data/ports/processes; teardown.
Report file:line, severity, CONFIRMED vs PLAUSIBLE, concrete failure scenario; list what you checked clean. Concise.
```
