# Router failure patterns (from a real session)

| Pattern | Symptom | Prevention |
|---|---|---|
| Promise without send | "I'll message you on Telegram" ×20, nothing sent for hours | Send in the same turn; write "Sent (id N)"; never future tense |
| Pointer instead of content | Human asks "give me that again" | Every ask complete; re-send in full |
| No closing action line | "so do I just need to wait?" | End every message with **What you need to do now:** |
| Stale plan, not real state | Telling the human to do steps they already did | Read git/tags/runs before every instruction |
| Wrong attribution | "Don't sign W6" for a bug that lived in W4 code | Find which commit/WU holds the defect before blocking a sign-off |
| Predicting OS behaviour | "Untick removes the Settings row" (it doesn't) | Label expectations as unverified; check vendor docs first |
| Trusting a PLAUSIBLE claim | Release run failed on a tool behaviour a reviewer guessed | Verify tool claims against source at the pinned version |
| Inherited model | Workers and reviewers silently on the default model | `--model` / `model:` on every spawn; check at start |
| Headless worker stops early | Exits after a status note or a question | Brief: one non-stop run; no mid-run questions |
| Brief path typo | Worker reads a missing file | One `$S` variable; `ls` every referenced brief |
| /tmp wiped | Briefs gone after reboot | Durable briefs dir; recreate from git after reboot |
| Router renamed | Workers report to a dead name | Re-announce name; briefs say "ignore older names" |
| Laundering a denial | Asking a peer to do a denied merge/copy/kill | Surface to the human with the exact command |
| Router doing the work | Opening the doc to address comments itself | Brief a worker; you only relay |
| "I can't" too early | Saying sessions can't be started | Investigate CLI options first (headless `-p` works) |
| Branch switched under an ask | "file does not exist" when the human opens the doc | Don't move a worktree's branch while an ask is pending |
| Extra worktree locks a branch | Worker can't switch to the epic | Remove temp worktrees right after use |
| Broken watcher | Background job "finished" instantly | No inner `&`; filter by head sha |
| Asking for run ids | "you find the run yourself" | `gh run list` yourself |
| fail-fast on first runs | One red row cancels the rest; one layer per run | `fail-fast: false` until the pipeline has run green once |
| Moving a pushed tag | (avoided) | Bump the version, new tag |
| Real user state in try-recipes | The human's config left in a state that later broke tests | Back up + restore; temp homes; track changes |
| Port collisions | Parallel e2e runs kill each other; orphan app holds the port | One e2e at a time; `lsof` first; human kills orphans if denied |
| Unanswered question repeated | Same gate question in many messages | Ask twice max; proceed on the stated default |
| Compaction | Habits (pushes, watchers) lapse after a summary | Re-read §3 of the skill after every summary |
