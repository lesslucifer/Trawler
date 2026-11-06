# Worker brief skeleton

Write to `<router dir>/briefs/<area>-<phase>-brief.md`; `ls` it before spawning. Every line exists because a worker once failed without it.

```markdown
# Brief: <E-CODE.Wn> <job> — fresh session (Opus)
Read first: <standing brief, abs path>, then <previous round's brief>. Router `<current name>` (older names: ignore).
ONE non-stop run; status messages are not stopping points; stop only at your final self-contained report.
No questions mid-run: decide reversible choices with a stated recommendation, or stop and report the options.
## Hard rules
- Never run `mr`; stage `cd <worktree> && cargo xtask mr ... --as vuluu` and stop.
- Never push, tag, dispatch workflows, `gh secret set`, or read ~/.tauri keys. Read-only `gh` is fine.
- Never amend/rebase a gated branch; never hand-write or copy a gate note.
- A classifier denial is not routed around: report the exact command for the owner.
- Never touch the real ~/Library/Application Support/dev.trawler.app, login items or system proxy; e2e only
  under a temp HOME, only if nothing listens on 127.0.0.1:4445 (`lsof`), one e2e at a time.
- Follow CLAUDE.md, docs/workflow.html, docs/testing-strategy.html, htmldoc; no comments unless essential; no estimates.
## State
Worktree <abs>, branch <b>, tip <full sha>. First: git branch --show-current, git log --oneline -6, git status --short.
Main is <sha> (check `git -C <main checkout> log --oneline -3 main`); merge main in with a merge commit if it moved.
Owner rulings: <Qn = x, ...>.
## Read first / evidence
<docs, cleaned CI log path, screenshots> · <failing step, error text, run id, file:line; verified vs hypothesis>
## Your job
1. Reproduce as tests red on the old code / plan commit first (T3, verify-doc green).
2. Root cause with evidence (third-party source at the pinned version); fix once where all callers route.
3. Audit the same bug class; predict the next CI layer before asking for a run.
4. Gates: gate0, vitest, notes write, preflight, code-review skill (code paths only), design-review (ui/src .tsx),
   verify-dod (only 8·manual-review red), actionlint on changed workflows.
5. Rule-bearing doc: leave it staged alone, stage `mr commit -m $'...'`; notes come in a fresh session after.
6. Try-it: a TESTED helper (free port, sample data), cd-prefixed `cargo run` steps, backup/restore of real state.
## Report to <router name>, then stop
What is signed · doc (abs path, branch confirmed) · full tip · ordered cd-prefixed owner commands · gate state ·
doc-only or how to try · no independent review (ui/src-tauri, login-service, .github, docs) · follow-ups.
```

| Variant | Add |
|---|---|
| CI fix round | per-job result, failing step, cleaned log path, hypotheses; "stage push + dispatch" |
| Step between | new tip D; notes write, cr skill (never copy), preflight, verify-dod on D |
| Epic close | DEC-3 archive, distill, epic result in docs/.archive, judgment file, epic cr vs main, copy archive to main checkout, stage confirm + merge + push |
| Review fix round | findings verbatim with severity; fix vs record; re-gate on the new tip |
| Parallel WU | create your own worktree; never touch another WU's worktree |
