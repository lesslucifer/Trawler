# Worker operations (zsh/bash, macOS)

`C` = claude binary (may not be on PATH; IDE-bundled path carries a version, never hard-code): `C=$(ps -axo command | grep -m1 -o '/[^ ]*native-binary/claude')` · `S` = router dir (briefs/, rulings.md, `<worker>.out`; mirror durably) · `WT` = worktree.

**Spawn** (same permission mode as yours, so idle notices reach you; always `--model`):
```sh
ls "$C" "$S/briefs/<brief>.md" && cd "$WT" && git branch --show-current && git status --short | wc -l && \
(nohup "$C" -p "You are a fresh worker session. Read $S/briefs/<brief>.md in full and do exactly what it says, in ONE non-stop run (status messages are not stopping points; stop only at your final self-contained report). Router is <router-name>. First action: git branch --show-current, git log --oneline -6, git status --short. Report with SendMessage to <router-name>." \
  -n <worker-name> --model <model-id> --permission-mode <your mode> > "$S/<worker-name>.out" 2>&1 &); echo started
ps -axo pid,command | grep -E 'claude.* -p ' | grep -v grep | grep -o -E '\-n [a-z0-9-]+ .*' | cut -c1-90   # name + --model
```
**Retire** (idle and clean only): `cd "$WT" && git status --short | wc -l` (0), then
`P=$(pgrep -f -- '-n <worker-name> ' | head -1); [ -n "$P" ] && kill "$P"`
**Wait** (never foreground `sleep`; one background command, no inner `&`, no `( … ) &`):
```sh
tip=<full sha>; until git notes --ref=<ref> show $tip >/dev/null 2>&1; do sleep 5; done; echo "SIGNED $tip"
until [ "$(gh run view <id> --json status -q .status)" = "completed" ]; do sleep 120; done
gh run view <id> --json conclusion,jobs -q '"run: \(.conclusion)", (.jobs[]|"\(.name): \(.conclusion)")'
until id=$(gh run list --workflow <wf>.yml --branch <br> --limit 1 --json databaseId,status,headSha \
  -q '.[0] | select(.status=="completed" and (.headSha|startswith("<sha8>"))) | .databaseId') && [ -n "$id" ]; do sleep 45; done
echo "RUN $id"; gh run view $id --json conclusion,jobs -q '.jobs[]|"\(.name): \(.conclusion) · failed: \([.steps[]|select(.conclusion=="failure")|.name]|join(" | "))"'
```
Monitor watches expire (30 min seen) and replay handled events: re-arm, dedupe.
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
