# Vikunja protocol (authoritative)

A local Vikunja instance is the router↔owner surface (E-VIKUNJA, owner's rulings in the epic plan). It replaces the Obsidian board (`board-protocol.md` on `epic/router-kanban`); that board's lessons carry over, its columns do not. Until E-VIKUNJA.W2 ships `cargo xtask vikunja`, the raw REST calls below are the interface; once it ships, its `--help` wins over this file for invocations.

## Where
| Thing | Value |
|---|---|
| Instance | Vikunja v2.8.0, official darwin-arm64 binary, bundled SQLite, no Docker. Home `~/.local/share/vikunja/` (`bin/vikunja` → `bin/vikunja-v2.8.0`, `config.yml`, `vikunja.db`, `files/`, `logs/`, `vikunja.out`); standing infra, never inside a repo |
| URL | `http://127.0.0.1:3456` (loopback only) · API `http://127.0.0.1:3456/api/v1` · web UI same origin |
| Liveness | `curl -sf http://127.0.0.1:3456/api/v1/info` (also proves `webhooks_enabled: true`) |
| Router token | `~/.config/trawler/vikunja.key` (600), user `trawler-router`; read by path only — never `cat`, print or copy it. Scoped to tasks · assignees · comments · task labels · relations · labels/projects read · project webhooks; no expiry |
| Passwords | `~/.config/trawler/vikunja-vuluu.password` (owner's web login) · `vikunja-trawler-router.password` (only to mint a replacement token); both 600, never printed |
| Fixed ids | project **Trawler Router** = 3 (views: List 9, Gantt 10, Table 11; the Kanban view was deleted on purpose) · label **needs-owner** = 1 · user `vuluu` = 1 (owner) · user `trawler-router` = 2 (router) · owner's saved filter **Needs me** = filter 1 (shows as project -2) |
| Registration | off; users are created with `bin/vikunja user create --config ./config.yml` from the home dir |

## Run it (plain detached process — no launchd, no login item)
```sh
V=~/.local/share/vikunja
cd $V && (env -u HTTP_PROXY -u HTTPS_PROXY -u http_proxy -u https_proxy -u ALL_PROXY nohup ./bin/vikunja web --config ./config.yml >> vikunja.out 2>&1 &)   # start
pgrep -lf 'bin/vikunja web --config'                                                                                              # status
pkill -f 'bin/vikunja web --config'                                                                                               # stop
```
It does not survive a reboot or logout, by design: nothing registers persistence on the owner's machine. The router's heartbeat is what keeps it up (Standing processes). Proxy variables are dropped so webhooks never route through a running Trawler. Logs: `$V/logs/`.

## Mapping
| Trawler thing | Vikunja object | Title | Description |
|---|---|---|---|
| Epic `E-<CODE>` | task in project 3 | `E-<CODE> <what>` | first line `Branch: epic/<slug>` once cut; one-line intent; plan path |
| WU `E-<CODE>.W<m>` | task, **subtask** of its epic (`relation_kind: subtask` on the epic) | `E-<CODE>.W<m> <what>` | first line `Branch: wu/<epic-slug>/<slug>` once cut |
| Owner task outside a WU (tag, CI dispatch, a decision) | task, subtask of its epic | `E-<CODE>:<slug> <what>` | — |
| Sequencing / cross-epic dependency | `relation_kind: blocked` on the later task, pointing at the earlier | — | — |

- Parent/child is hierarchy only; blocking is order only. Never infer order from the hierarchy or hierarchy from a block. Vikunja stores both reciprocally (`subtask`↔`parenttask`, `blocked`↔`blocking`) and enforces neither: the router checks a task's `blocked` list is all `done` before starting it.
- One project, no buckets. Position, view, sort order and Gantt dates carry no meaning; nothing reads them.
- `done = true` is set by the router only after checking git: a WU when merged into its epic, an epic when merged into `main` and pushed. Comment the merge sha in the same pass.
- **Task before branch** (carried from E-KANBAN): create the task → `git worktree add … -b <branch>` → write the `Branch:` line (same turn) → spawn the worker. W4's orphan check is the backstop that detects a skipped step, never prevention.

## The needs-owner signal (the only owner queue)
The owner's queue is exactly: `done = false && assignees in vuluu && labels in 1` — the **Needs me** saved filter. Membership changes only through an explicit label or assignee change — never a position.

| Step | Who | Exact action | Means |
|---|---|---|---|
| 1 Ask | router | comment with the whole ask → assign `vuluu` → add `needs-owner`, in that order | the label lands last, so the ask is already there when the webhook fires |
| 2 Answer | owner | comments as needed, then **removes `needs-owner`** | "I'm done with this ask". The answer is the owner's comments since the ask; none = the ask's stated default |
| 2' Command done | owner runs the staged `mr confirm` / merge / push / tag | nothing in Vikunja needed | the router's git watcher sees it, removes the label itself, comments `seen <sha8>` |
| 3 Close the ask | router | reads the answer, unassigns `vuluu`, comments what it will do | the task leaves the owner's queue for good |

The ask comment is self-contained, rewritten whole for every new ask (never "see above"): what is asked · read-first path (`cd <abs wt> && open <doc>`) · full sha + gate state + what had no independent review · ordered `cd <abs> && <cmd>` in a ```sh fence · what the owner will see · how the router detects done · for a ruling: each option with its cost, the recommendation, the **default** if the owner just removes the label.

| Owner does | Router reads it as |
|---|---|
| removes `needs-owner` | the only "answered/proceed" signal; act on the comments, else the default |
| comments while the label is still on | not a signal: reply if it is a question, take no action on the ask |
| comments on a task with no open ask | an instruction to read and acknowledge by comment; anything needing a sign-off back becomes a new ask (step 1) |
| marks a task `done` | archival only, never approval: if an ask was open, comment `closed without an explicit approval signal — pending ask discarded, no action taken`, remove the label, unassign |
| re-adds `needs-owner` | the owner wants the router's attention on that task: read its newest comments |

Every webhook payload carries `doer`; the router ignores events whose doer is `trawler-router` (its own writes). Removing only the assignee and leaving the label is never valid router state: always label first, assignee after.

## Standing processes
Same discipline the board watcher ended up with (E-KANBAN postmortem): watching is a standing responsibility from session start, not a per-ask action.

| Process | Owner | Every heartbeat tick | When found dead |
|---|---|---|---|
| Vikunja server | router | `curl -sf …/api/v1/info` | relaunch (Run it) the same tick; ping Telegram only if the relaunch fails |
| Webhook listener (W3) | router | the liveness check W3 defines | relaunch the same tick, then run a reconciliation pass |

Webhooks are latency; the API is the truth. Vikunja delivers each webhook once and never retries (≥ 400 or a 30 s timeout is lost), so after any relaunch, and at session start, the router reads the **Needs me** query and each open task's newest comments, and acts on any state it has not handled yet.

## Webhook capability (for W3)
| Fact | Value |
|---|---|
| Enabled | `webhooks.enabled: true` (default; `/info` → `webhooks_enabled: true`) |
| Loopback targets | need `outgoingrequests.allownonroutableips: true` (set in `config.yml`; the old `webhooks.allownonroutableips` is deprecated) |
| Registration | per project: `PUT /projects/3/webhooks` `{target_url, secret, events[]}`; none configured yet |
| Signature | header `X-Vikunja-Signature` = HMAC-SHA256 of the raw body keyed with the webhook `secret`; the listener verifies it before parsing |
| Delivery | once, no retry; 30 s timeout (`webhooks.timeoutseconds`) |
| Body | `{event_name, time, data: {doer, project, task, …}}`; `data.task` carries full `labels[]` and `assignees[]` |
| Events seen in a live probe | assign → `task.assignee.created` then `task.updated` · label add or remove → `task.updated` only (there is **no label event**) · comment → `task.comment.created` (`data.comment`) · done → `task.updated` |

So the listener matches on state, not event names: `task.updated` / `task.assignee.created` whose task has `vuluu` in `assignees` and `needs-owner` in `labels` = an ask raised; a transition to no `needs-owner` with doer `vuluu` = an answer; `task.comment.created` with doer `vuluu` = an owner comment.

## Raw API (until `cargo xtask vikunja`)
```sh
API=http://127.0.0.1:3456/api/v1; H=(-H "Authorization: Bearer $(cat ~/.config/trawler/vikunja.key)" -H 'Content-Type: application/json')
curl -s -X PUT  $API/projects/3/tasks        "${H[@]}" -d '{"title":"E-X.W1 …","description":"Branch: wu/x/y"}'   # create
curl -s -X PUT  $API/tasks/<epic>/relations  "${H[@]}" -d '{"other_task_id":<wu>,"relation_kind":"subtask"}'
curl -s -X PUT  $API/tasks/<later>/relations "${H[@]}" -d '{"other_task_id":<earlier>,"relation_kind":"blocked"}'
curl -s -X PUT  $API/tasks/<id>/comments     "${H[@]}" -d '{"comment":"…"}'
curl -s -X PUT  $API/tasks/<id>/assignees    "${H[@]}" -d '{"user_id":1}'      # DELETE …/assignees/1 to unassign
curl -s -X PUT  $API/tasks/<id>/labels       "${H[@]}" -d '{"label_id":1}'     # DELETE …/labels/1 to remove
curl -s         $API/tasks/<id>              "${H[@]}"                          # read (related_tasks, labels, assignees)
curl -s         $API/tasks/<id>/comments     "${H[@]}"
curl -s        "$API/projects/3/views/9/tasks?filter=done%20%3D%20false" "${H[@]}"   # open tasks
```
`POST /tasks/<id>` replaces the whole task: a partial body clears assignees and other fields (seen in the probe). Read, modify, then post the full object. Change labels and assignees through their own endpoints only. The SQLite file is never written directly.

v2.8.0 also exposes an `mcp` route group (`GET /routes`); it is not used yet.
