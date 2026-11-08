# Vikunja protocol (authoritative)

A local Vikunja instance is the router↔owner surface (E-VIKUNJA, owner's rulings in the epic plan). It replaces the Obsidian board (`board-protocol.md` on `epic/router-kanban`); that board's lessons carry over, its columns do not. `cargo xtask vikunja` is the interface (run it bare for its usage, which wins over this file for invocations); raw REST only for what it lacks (end of file). `X` = a checkout that has it: `$M/Trawler` once E-VIKUNJA is on `main`, until then `$M/Trawler-vikunja` (`epic/router-vikunja`).

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
- **Task before branch** (carried from E-KANBAN): create the task → `git worktree add … -b <branch>` → write the `Branch:` line (same turn) → spawn the worker. `cargo xtask vikunja check` (every heartbeat tick, Standing processes) is the backstop that detects a skipped step, never prevention.
- The `Branch:` line is a line starting `Branch:` (plain text, or a `<p>` after the owner edits the description in the web UI); it is the only task ↔ branch link. `put --description` replaces the whole description: send the full text.

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

## The lifecycle as CLI calls (`cd $X && cargo xtask vikunja …`; text from a file on stdin with `-`)
| Step | Call |
|---|---|
| new epic · new WU | `create --title 'E-<CODE> <what>' --description -` · `create --title 'E-<CODE>.W<m> <what>' --parent <epic id> [--relation blocked-by:<id>]` |
| branch cut | `put <id> --description -` (full text, `Branch: <branch>` first) |
| 1 Ask | `put <id> --comment - --assignee vuluu --label needs-owner` (the CLI applies comment → assign → label whatever the flag order) |
| 2' command seen + 3 close | `put <id> --comment 'seen <sha8>' --unlabel needs-owner --unassign vuluu` (unlabel before unassign, always) |
| 3 Close the ask | `put <id> --comment - --unassign vuluu` |
| owner marked done with an ask open | `put <id> --comment 'closed without an explicit approval signal — pending ask discarded, no action taken' --unlabel needs-owner --unassign vuluu` |
| merged | `put <id> --done --comment 'merged <sha8> into <target>'` |
| read · owner's queue · all open · backstop | `read <id>` · `show --needs-owner` · `show` · `check` |

Every webhook payload carries `doer`; the router ignores events whose doer is `trawler-router` (its own writes). Removing only the assignee and leaving the label is never valid router state: always label first, assignee after.

## Standing processes
Same discipline the board watcher ended up with (E-KANBAN postmortem): watching is a standing responsibility from session start, not a per-ask action.

| Process | Owner | Every heartbeat tick | When found dead |
|---|---|---|---|
| Vikunja server | router | `curl -sf …/api/v1/info` | relaunch (Run it) the same tick; ping Telegram only if the relaunch fails |
| Webhook listener | router | `curl -sf -m 5 http://127.0.0.1:3457/` → `ok`; its event file under a Monitor | relaunch (Run the listener) the same tick, after the Vikunja server is up; its startup reconcile replays what the gap hid; re-arm the Monitor |
| Orphan check | router | after both rows above: `cd $X && cargo xtask vikunja check; echo EXIT $?` | — (one-shot) |

`cargo xtask vikunja check` lists `git worktree list` of the checkout's repo and every project-3 task (done ones too: a merged WU's worktree outlives its task) and prints one JSON line per finding; exit 0 = none (nothing printed) · 1 = findings · 2 = error on stderr (Vikunja down: relaunch it first).

| Line | Means | Router does |
|---|---|---|
| `{"branch":"wu/x/y","event":"orphan","worktree":"/abs"}` | an `epic/*` or `wu/*` worktree no task's `Branch:` names | create its task now (a WU under its epic) with the `Branch:` line, or an ask when it cannot tell what it is; one Telegram ping per tick naming the tick's orphans |
| `{"branch":"wu/x/y","event":"missing-worktree","task_id":9,"title":"…"}` | an open task whose branch has no worktree | git: merged → `put --done` + merge sha; a wrong `Branch:` line → fix it; else an ask |

A finding repeats every tick until fixed. JSON keys print in alphabetical order (listener lines too): match on `"event":"<name>"`, never on the line's prefix.

Keeping the server and the listener alive and running the check is the router's **standing responsibility for the whole session**, exactly like the board watcher was: started at session start, checked every heartbeat tick, relaunched the moment it is found dead — never a one-time check, never only when an ask is open. A dead listener is silent, not loud: nothing else tells the router an owner answered.

Webhooks are latency; the API is the truth. Vikunja delivers each webhook once and never retries (≥ 400 or a 30 s timeout is lost); the listener covers that by reconciling against the API at every start and every 60 s.

## Run the listener
```sh
L=~/.local/share/vikunja
cd $X && (nohup cargo xtask vikunja webhook-listen >> $L/listener.events 2>> $L/listener.log &)   # start
curl -sf -m 5 http://127.0.0.1:3457/                                                                           # status: ok
pkill -f 'xtask vikunja webhook-listen'                                                                   # stop
tail -n0 -F $L/listener.events                                                                            # watch (Monitor): one line = one notification
```
At start it binds `127.0.0.1:3457`, replaces its own project-3 webhook (same `target_url`; never a duplicate), reconciles, prints `listening`, then serves until killed. Files: secret `~/.config/trawler/vikunja-webhook-secret` (600, created on first start, re-sent at every registration; never printed), state `~/.config/trawler/vikunja-listener.json` (per task: asking, last comment id seen). Rejected deliveries are logged to stderr. `cargo xtask vikunja` has the flags (`--port`, `--state`, `--secret-file`, `--poll`).

| Line (stdout, one JSON object each) | Means | Router does |
|---|---|---|
| `{"event":"needs-owner","task_id":9,"title":"…","by":"…"}` | task entered the owner's queue (open ∧ `vuluu` ∧ `needs-owner`); at startup, every ask still pending is re-announced (no `by`) | ping the owner unless it already pinged for this ask |
| `{"event":"answered",…}` | left the queue with the label gone — the only answer | step 3, unless `by` is `trawler-router` (its own 2') |
| `{"event":"closed",…}` | left the queue with the label still on (marked done or unassigned), or the asking task was deleted (then no `title`) | the done row of the table above |
| `{"event":"comment","task_id":9,"title":"…","by":"vuluu","text":"…"}` | a comment not written by `trawler-router` (text is Vikunja's HTML) | reply / acknowledge — never a lifecycle step |
| `{"event":"unreachable","error":"…"}` · `{"event":"reachable"}` | the 60 s reconcile failed / works again (printed once per outage) | relaunch the Vikunja server |
| `{"event":"listening","port":3457,"webhook_id":4}` | startup finished; lines before it are the reconcile's | — |

Every delivery is only a trigger: the listener re-reads the task and its comments from the API and diffs them against its state, because deliveries arrive out of order and after later writes. `by` is the doer of the delivery that showed the change. Reconcile scope: open tasks of project 3 plus tasks last seen asking; a comment on a done task while the listener was down is not replayed.

## Webhook capability
| Fact | Value |
|---|---|
| Enabled | `webhooks.enabled: true` (default; `/info` → `webhooks_enabled: true`) |
| Loopback targets | need `outgoingrequests.allownonroutableips: true` (set in `config.yml`; the old `webhooks.allownonroutableips` is deprecated) |
| Registration | per project: `GET`/`PUT /projects/3/webhooks` `{target_url, secret, events[]}`, `DELETE /projects/3/webhooks/<id>`; the router token may; responses never echo the secret. The listener owns its registration |
| Signature | header `X-Vikunja-Signature` = lowercase hex HMAC-SHA256 of the raw body keyed with the webhook `secret`; the listener verifies it before parsing |
| Delivery | once, no retry; 30 s timeout (`webhooks.timeoutseconds`) |
| Body | `{event_name, time, data: {doer, project, task, …}}`; `data.task` carries full `labels[]` and `assignees[]` |
| Events seen in a live probe | assign → `task.assignee.created` then `task.updated` · label add or remove → `task.updated` only (there is **no label event**) · comment → `task.comment.created` (`data.comment`) · done → `task.updated` |

Deliveries are sent asynchronously: they arrive out of order and carry the task as read at send time. So the listener matches on state, not event names: `task.updated` / `task.assignee.created` whose task has `vuluu` in `assignees` and `needs-owner` in `labels` = an ask raised; a transition to no `needs-owner` with doer `vuluu` = an answer; `task.comment.created` with doer `vuluu` = an owner comment.

## Raw API (only for what `cargo xtask vikunja` lacks: a relation on an existing task other than blocked-by/blocks, deleting a task)
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
