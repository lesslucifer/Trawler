# Worker brief skeleton (Trawler)

Distilled from the briefs that worked (`pkg2-w3-round5`, `bg-service-w3`,
`pkg2-epic-notes`, `rc-hotfix2`, `relfix-w2-inspect-form`). Each line exists
because a worker once failed without it. Write to
`<router dir>/briefs/<area>-<phase>-brief.md`, then `ls` it before spawning.

```markdown
# Brief: <E-CODE.Wn> <one-line job> — fresh session (Opus)

Read first: <standing brief for this stream, absolute path> (standing rules), then <previous round's brief> for history.
Router `<current router name>` (older briefs name <old names>: ignore). ONE non-stop run; status messages are not
stopping points; stop only at your final self-contained report. Do not ask questions mid-run: decide reversible
choices with a stated recommendation, or stop and report the options.

## Hard rules
- Never run `mr`; stage `cargo xtask mr ... --as vuluu` with a `cd <worktree> &&` prefix and stop.
- Never push, tag, dispatch workflows, `gh secret set`, or read ~/.tauri keys. Read-only `gh` is allowed.
- Never amend or rebase a gated branch; never hand-write or copy a gate note (a copy was denied before).
- If the permission classifier denies something, do not route around it: report the exact command for the owner.
- Never touch the owner's real ~/Library/Application Support/dev.trawler.app, login items or system proxy;
  e2e only under a temp HOME, only if nothing listens on 127.0.0.1:4445 (`lsof` first), one e2e at a time.
- Follow CLAUDE.md, docs/workflow.html, docs/testing-strategy.html; docs per the htmldoc skill; no code comments
  unless essential; no work estimates.

## State
Worktree <abs path>, branch <branch>, tip <full sha>. First: git branch --show-current, git log --oneline -6,
git status --short; confirm clean. Main is <sha> (`git -C <main checkout> log --oneline -3 main`: the owner merges
and pushes as they go, so check the real state). Merge main into the epic with a merge commit if it moved.
Rulings already made by the owner: <Qn = x, ...>.

## Read first
<plan/result/decision docs, logs (cleaned CI log path), owner screenshots (absolute paths)>

## The problem / evidence
<exact failing step, error text, run id, file:line; what is verified vs hypothesis>

## Your job
1. <reproduce as tests red on the old code / plan commit first (T3, verify-doc green)>
2. <find the root cause with evidence (read third-party source at the pinned version); fix once where all callers route>
3. <audit for the same class of bug / predict the next CI layer before asking for a run>
4. Gates: gate0, vitest, `cargo xtask notes write ...`, `cargo xtask preflight ...`, the code-review skill (isolated
   finders; only if the diff changes code paths), design-review if a ui/src .tsx changed, `cargo xtask verify-dod ...`
   (only 8·manual-review may stay red),
   actionlint on changed workflows.
5. Rule-bearing doc (T0/T1/DEC rule change): leave it staged alone and stage `mr commit -m $'...'`; the notes step
   comes in a fresh session after the owner signs.
6. Try-it recipe: a TESTED helper (local server on a free port, sample data) and cd-prefixed `cargo run` steps,
   with backup/restore of any real state it touches.

## Report to <router name>, then stop
What is being signed (plain words) · doc to read (absolute path; confirm the branch) · full tip sha · ordered
cd-prefixed owner commands (mr commit if any, mr confirm, merge, push/tag) · gate state · doc-only or how to try it ·
what had no independent review (ui/src-tauri, login-service, .github, docs) · follow-ups recorded.
```

## Variants

| Phase | Add |
|---|---|
| CI fix round N | the run's per-job result, failing step, cleaned log path, hypotheses to verify; "stage push + dispatch" |
| Step between (after an owner `mr commit`) | new tip D; `notes write` on D; cr via the skill on D (never copy); preflight; verify-dod |
| Epic close | archive per DEC-3, roadmap/T2 distill, epic result in docs/.archive, epic judgment file, epic-scope cr vs main, copy archive to main checkout, stage `mr confirm epic` + merge + push |
| Fix round after an independent review | the findings verbatim with severity; which to fix, which to record; re-gate on the new tip |
| Parallel WU | create your own worktree (`git worktree add ...`); never touch the other WU's worktree or files |
