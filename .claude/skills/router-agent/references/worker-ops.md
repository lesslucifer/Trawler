# Worker operations (zsh/bash, macOS)

`C` = absolute claude path (the project skill names it; `command -v claude` can fail in the router's shell; else `C=$(ps -axo command | grep -m1 -o '/[^ ]*native-binary/claude')`) · `S` = router dir (briefs/, rulings.md, `<worker>.out`; mirror durably) · `WT` = worktree.

**Spawn** (same permission mode as yours, so idle notices reach you; always `--model`):
```sh
ls "$C" "$S/briefs/<brief>.md" && cd "$WT" && git branch --show-current && git status --short | wc -l && \
(nohup "$C" -p "You are a fresh worker session. Read $S/briefs/<brief>.md in full and do exactly what it says, in ONE non-stop run (status messages are not stopping points; stop only at your final self-contained report). Router is <router-name>. First action: git branch --show-current, git log --oneline -6, git status --short. Report with SendMessage to <router-name>." \
  -n <worker-name> --model <model-id> --permission-mode <your mode> > "$S/<worker-name>.out" 2>&1 &); echo started
ps -axo pid,command | grep -E 'claude.* -p ' | grep -v grep | grep -o -E '\-n [a-z0-9-]+ .*' | cut -c1-90   # name + --model
n=0; until pgrep -f -- '-n <worker-name> ' >/dev/null || [ $n -ge 10 ]; do sleep 1; n=$((n+1)); done; pgrep -f -- '-n <worker-name> ' && echo RUNNING || { echo DEAD; tail -5 "$S/<worker-name>.out"; }
```
Comment a worker on its task only after `RUNNING` (run the check in the background).
**Retire** (idle and clean only): `cd "$WT" && git status --short | wc -l` (0), then
`P=$(pgrep -f -- '-n <worker-name> ' | head -1); [ -n "$P" ] && kill "$P"`
**Watch** every staged owner action (named in its ask's `Watching:`; on fire: verify, label off + `seen <sha8>`, close the ask) (never foreground `sleep`; one background `until <probe>; do sleep N; done; echo <EVENT>` per action, no inner `&`, no `( … ) &`):
| Owner action | Probe succeeds when |
|---|---|
| signs (git note) · commits | `git notes --ref=<ref> show <tip>` · `[ "$(git rev-parse <branch>)" != <old tip> ]` |
| merges · pushes · tags | `git merge-base --is-ancestor <tip> <target>` · `[ "$(git ls-remote origin refs/heads/<b> \| cut -f1)" = <tip> ]` · `[ -n "$(git ls-remote --tags origin <tag>)" ]` |
| publishes a draft · comments a doc | `[ "$(gh release view <tag> --json isDraft -q .isDraft)" = false ]` · open-comment count ≠ last seen |
```sh
until [ "$(gh run view <id> --json status -q .status)" = "completed" ]; do sleep 120; done
gh run view <id> --json conclusion,jobs -q '"run: \(.conclusion)", (.jobs[]|"\(.name): \(.conclusion)")'
until id=$(gh run list --workflow <wf>.yml --branch <br> --limit 1 --json databaseId,status,headSha \
  -q '.[0] | select(.status=="completed" and (.headSha|startswith("<sha8>"))) | .databaseId') && [ -n "$id" ]; do sleep 45; done
echo "RUN $id"; gh run view $id --json conclusion,jobs -q '.jobs[]|"\(.name): \(.conclusion) · failed: \([.steps[]|select(.conclusion=="failure")|.name]|join(" | "))"'
```
Monitor watches expire (30 min seen) and replay handled events: re-arm, dedupe.
**Tracker listener** (always one running; start, liveness probe, event file and exact commands: the project's tracker protocol, e.g. `trawler-router/references/vikunja-protocol.md` "Run the listener"): start it detached with stdout appended to its event file, then watch that file with Monitor (`tail -n0 -F <events>`: one line = one notification) and act on each line (SKILL.md "Tracker loop" table). A Monitor that expires or replays is re-armed and deduped; a listener found dead is relaunched at once (its startup reconcile replays the gap, re-announcing pending asks: ping only asks never pinged).
**Heartbeat**: CronCreate `*/10 * * * *`, recurring (session-only, fires only while idle, expires after 7 days: re-create), self-contained prompt: "Router heartbeat: 0 tracker: server liveness → dead: relaunch now, ping only if the relaunch fails; listener liveness → dead: relaunch now (after the server), re-arm its event Monitor; event Monitor missing → re-arm. Then the orphan check → per `orphan` create the missing task with its `Branch:` line (or an ask when unknown), then ONE ping naming those branches; per `missing-worktree` merged → done, wrong line → fix, else an ask; exit 2 → server first. 1 workers: `ps` names + --model vs expected; dead → relaunch from its brief (it keeps and checks uncommitted work), confirm RUNNING; stalled (tip and tree unchanged 3 beats, no report) → SendMessage a ping, still silent next beat → relaunch. 2 worktrees: branch, tip, dirty, notes on tip. 3 pending owner actions, runs, doc comments (relay to the worker), unrelayed worker reports; re-arm missing watchers. 4 advance anything unblocked (next WU, step-between worker, stage commands as an ask). 5 every change goes to its task (a comment, an ask, done); nothing to chat; no ping except step 0's and a `needs-owner` line's. Never run reserved commands."
**Read failures / third-party source** (read-only):
```sh
gh run view <id> --log-failed | sed 's/\x1b\[[0-9;]*[mK]//g' > "$S/run-<id>.log"; grep -a -n -E '(error|warning)(\[[A-Za-z0-9_]+\])?:|could not compile|##\[error' "$S/run-<id>.log" | head -40
R=$(gh repo view --json nameWithOwner -q .nameWithOwner); gh api repos/$R/actions/jobs/<job-id>/logs > "$S/job.log"
gh api "repos/<owner>/<repo>/contents/<path>?ref=<tag>" -q .content | base64 -d
```

| Shell pitfall | Fix |
|---|---|
| macOS `sed -i 's/…//'` → "undefined label" | `sed -i '' …` |
| jq `created:createdAt` → "function not defined" | `created: .createdAt` |
| `git rev-parse --short A B` → "Needed a single revision" | one ref per call |
| zsh `--include=*.rs` → "no matches found" | quote: `--include='*.rs'` |
| `head -$n`, n=0 → "illegal line count" | guard n, or `sed -n` |
| `-m "…\n\n…"` → literal `\n` in the commit | `-m $'subject\n\nbody'`, `\'` |
| `cmd \| grep -q` under pipefail → exit 141 | `v=$(cmd)`; compare with `[ ]` |
