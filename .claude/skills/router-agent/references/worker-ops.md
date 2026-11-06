# Worker operations: command templates

All templates are zsh/bash on macOS. Replace `<...>`. Run the `ls` checks; they
are there because each one caught a real failure.

## Variables (set once per command; never retype paths)

```sh
C=<absolute path to the claude binary>      # `claude` may not be on PATH (IDE-bundled binary); find it with ps on a live session
S=<router dir>                              # briefs/, rulings.md, <worker>.out logs; the session scratchpad is wiped on reboot, so copy briefs somewhere durable too
WT=<absolute worktree path>
```

Find the binary: `ps -axo command | grep -m1 -o '/[^ ]*native-binary/claude'` (an IDE-bundled binary's path carries the extension version, so never hard-code it).

## Spawn a fresh headless worker

```sh
ls "$S/briefs/<brief>.md" && cd "$WT" && git branch --show-current && git status --short | wc -l && \
(nohup "$C" -p "You are a fresh worker session. Read $S/briefs/<brief>.md in full and do exactly what it says, in ONE non-stop run (status messages are not stopping points; stop only at your final self-contained report). Router is <router-name>. First action: git branch --show-current, git log --oneline -6, git status --short. Report with SendMessage to <router-name>." \
  -n <worker-name> --model <model-id> --permission-mode <your mode> > "$S/<worker-name>.out" 2>&1 &); echo started
```

- Always pass `--model` (otherwise it inherits the user's default, which was Sonnet in the source session).
- Same permission mode as yours, so idle notices reach you.
- Verify: `ps -axo pid,command | grep -E 'claude.* -p ' | grep -v grep | grep -o -E '\-n [a-z0-9-]+ .*' | cut -c1-90` shows the name and the `--model` flag; the peer list shows the name.

## Retire a worker

```sh
cd "$WT" && git status --short | wc -l        # must be 0 (or the work is committed)
P=$(pgrep -f -- '-n <worker-name> ' | head -1); echo "pid $P"; [ -n "$P" ] && kill "$P"
```

Only when idle (finished its turn) and clean. Never kill mid-turn or with uncommitted changes; resume uncommitted work with a fresh worker told to keep and check it.

## Wait without polling

- Never `sleep N; check` in the foreground (blocked by the harness, and wasteful).
- Condition on local state (e.g. a signature note appears): a Monitor-style until-loop, re-armed at expiry (30 min in the source session); expect duplicate notices for already-handled events.

```sh
tip=<full sha>; until git notes --ref=<ref> show $tip >/dev/null 2>&1; do sleep 5; done; echo "SIGNED $tip"
```

- Long CI/release runs: ONE background command, no inner `&`, no `( ... ) &`:

```sh
until [ "$(gh run view <id> --json status -q .status)" = "completed" ]; do sleep 120; done
gh run view <id> --json conclusion,jobs -q '"run: \(.conclusion)", (.jobs[]|"\(.name): \(.conclusion)")'
```

- Run not created yet: match the exact head sha, not just "latest on branch":

```sh
until id=$(gh run list --workflow <wf>.yml --branch <br> --limit 1 --json databaseId,status,headSha \
  -q '.[0] | select(.status=="completed" and (.headSha|startswith("<sha8>"))) | .databaseId') && [ -n "$id" ]; do sleep 45; done
echo "RUN $id"; gh run view $id --json conclusion,jobs -q '.jobs[]|"\(.name): \(.conclusion) · failed: \([.steps[]|select(.conclusion=="failure")|.name]|join(" | "))"'
```

## Read a failed job (read-only)

```sh
gh run view <id> --log-failed | sed 's/\x1b\[[0-9;]*[mK]//g' > "$S/run-<id>.log"; grep -a -n -E '(error|warning)(\[[A-Za-z0-9_]+\])?:|could not compile|##\[error' "$S/run-<id>.log" | head -40
R=$(gh repo view --json nameWithOwner -q .nameWithOwner); gh api repos/$R/actions/jobs/<job-id>/logs > "$S/job.log"
```

Third-party source at a pinned version (read-only): `gh api "repos/<owner>/<repo>/contents/<path>?ref=<tag>" -q .content | base64 -d`.

## Shell pitfalls seen

| Pitfall | Fix |
|---|---|
| `sed -i 's/..//' f` on macOS → "undefined label" | `sed -i '' 's/..//' f` |
| jq `created:createdAt` → "function not defined" | `created: .createdAt` |
| `git rev-parse --short A B` → "Needed a single revision" | one ref per `--short`, or drop `--short` |
| zsh `grep --include=*.rs` → "no matches found" | quote globs: `--include='*.rs'` |
| `head -$n` with n=0 → "illegal line count" | guard n, or use `sed -n` |
| commit message with `"...\n\n..."` lands a literal `\n` | `-m $'subject\n\nbody'`, apostrophes as `\'` |
| run step `cmd | grep -q` under `set -o pipefail` → exit 141 (SIGPIPE) | capture first: `v=$(cmd)`; compare with `[ ]` |
| checking a merge while its hooks still run | wait until `.git/MERGE_HEAD` is gone and `git log -1` is the merge |
