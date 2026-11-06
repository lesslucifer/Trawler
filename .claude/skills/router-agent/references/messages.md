# Message templates

## Ask for a sign-off (terminal)

```
**<Item> is ready for you to sign.** I checked it myself: tip `<full sha>` on `<branch>`, tree clean, <notes/gates> on the tip. Only your signature is red.

**What you're signing:** <2–5 bullets in user terms>
**Doc only** | **You can try it in the real app** (steps below)
**Gate state:** <green list> · **Not independently reviewed:** <paths or "none">

**Read first:**
cd <abs worktree> && open <doc path>

**Try it** (helper tested by me/the worker; restore steps included):
1. ...

**Run these in order:**
1. cd <abs path> && <sign command>
2. cd <abs path> && <merge command>

**Next:** <what you or the workers do after>

**What you need to do now:**
1. ...
```

## Ask for a decision

```
**<Worker/item> needs one decision.** <one-line context>

| Option | What it means | Cost |
|---|---|---|
| **A** (my pick) | ... | ... |
| B | ... | ... |

**Default if you don't answer:** A. Reply "A" or "B".
**What you need to do now:** 1. Reply A or B.
```

## Status board

```
| Worker | Doing | You'll get |
|---|---|---|
**Waiting on you:** <list or nothing> · **Running without you:** <list>
**What you need to do now:** nothing. I'll message you on <channel> when <event>.  (only after arming the watcher)
```

## Push (Telegram) message

```
✍️ Sign-off ready: <item>
📄 Doc only | 🧪 Try in app
<one line what it is>

Sign
cd <abs path> && <command>

Then: <one line next>
```

≤ ~10 lines, plain text, every command complete. Same turn as the terminal message; report the message id.

## Independent review prompt (read-only subagent, strongest model)

```
Independent READ-ONLY review (very thorough) of <what>, which the automated review gate does not bind.
Worktree <abs>, branch <b>, tip <sha>; base <sha>. Do not edit, run <reserved commands>, push, tag,
or execute downloaded binaries; git/grep/read and read-only gh api only.
Scope: `git diff <base> HEAD -- <paths>` plus <named files>. Rules to check against: <decision docs>.
Hunt: (1) <exit/quit paths that skip a safety check> (2) <unsafe/FFI soundness and confinement>
(3) <secret/key exposure: only in the step that needs it, never logged, not reachable by third-party code>
(4) <workflow traps: injection via tag/ref in run:, unpinned actions, caches, pipefail, artifact naming>
(5) <tests that can touch the user's real data, ports, processes; teardown on every failure path>
Report each finding with file:line, severity, CONFIRMED vs PLAUSIBLE, and a concrete failure scenario;
list what you checked and found clean. Concise.
```
