---
name: code-review
description: Run Trawler's multi-agent code review — a read-only run over a caller-supplied diff scope; mechanical gate, five isolated finders, adversarial verifiers, verdict, and a YAML evidence note on the reviewed tip. Use when a Working Unit or epic completes, before any merge, or to review recent commits. The caller defines the scope (WU branch vs its epic branch, epic vs main, or recent commits) — never assume a base.
---

# Code review — orchestration

A review run is **read-only**: no agent in it edits the tree or commits.
Fixing belongs to the session that triggered the run (§7). What to review
lives in `rubric.md` next to this file — finders read it; this file only
sequences the run.

## 1 · Resolve scope

- Argument `<base>..<head>` (e.g. `epic/capture..wu-pagination`,
  `main..epic/capture`), or a single ref meaning `<ref>^..<ref>`.
- No argument: `HEAD^..HEAD` plus any uncommitted changes.
- **Never** widen the scope on your own — the caller owns it (usually a WU
  branch against its parent epic branch, or an epic against `main`).
- Pin full SHAs now: `BASE=$(git merge-base <base> <head>)` and
  `HEAD_R=$(git rev-parse <head>)`. Merge-base, not the base tip — commits
  that landed on the base branch after the fork must not pollute the diff.
- The reviewed diff is `git diff BASE..HEAD_R`, plus `git diff HEAD_R` when
  the working tree is in scope.
- Optional, repeatable: `--waive "<finding summary>: <reason>"` — the
  trigger's waiver declarations, applied at the verdict (§5).

## 2 · Gate 0 — abort if red

Run in order; any failure ⇒ stop, report it to the caller, spawn no agents,
write no evidence:

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo nextest run --all` (fallback `cargo test --all`)

## 3 · Find — five isolated finders, in parallel

Spawn five subagents **in a single message** (fresh context each), one per
dimension in `rubric.md`. Each finder prompt contains exactly:

- its dimension name, and the instruction to read
  `.claude/skills/code-review/rubric.md` first — its dimension row, the
  principles, and the exclusions;
- the scope (`BASE..HEAD_R`, plus working tree if in scope) — the finder runs
  `git diff` itself and may read any file in the repo;
- the return format: a list of candidate findings, each
  `{location: file:line, claim: <one falsifiable sentence>, severity: blocker|important|nit}`.

## 4 · Verify — one fresh verifier per candidate, in parallel

A verifier receives **only** the finding and the scope — never the finder's
reasoning. Its prompt:

- try to **refute** the claim against actual code behavior: read the full
  surrounding code, trace callers and data flow, run a targeted test when
  cheap — never judge from the diff hunk alone;
- check `docs/decisions/` (when it exists) before confirming a finding that
  "fixes" something deliberate — the oddity may be load-bearing;
- resolve uncertainty to false-positive — with no human filter downstream, a
  false positive costs more than a missed nit; recall comes from five
  overlapping finders;
- return `{verdict: real|false-positive, severity, evidence: <why>}`.

Refuted findings are dropped from the verdict; mention them briefly when
returning to the caller (§7) so finder quality stays visible.

## 5 · Verdict

- Match the trigger's `--waive` declarations against verified findings;
  record each waiver with its reason (citing a `docs/decisions/` record where
  one applies).
- **PASS** = Gate 0 green and zero verified findings left unwaived.
  Anything else is **FAIL** — evidence is written either way.

## 6 · Evidence — YAML note on the reviewed tip

- `DIFF_SHA=$(git diff BASE..HEAD_R | shasum -a 256 | cut -d' ' -f1)`
  (pipe the working-tree diff in as well when it was in scope).
- The note body is one YAML document, contract `cr/v1`. A DoD-check agent
  parses it at done-gates: a Working Unit is not merge-ready and an epic is
  not complete without a `PASS` note whose `diff_sha256` matches the tip
  being merged — so a later rewrite of the branch visibly orphans its
  evidence.

```yaml
schema: cr/v1
scope: "WU capture-pagination -> epic/capture"   # the caller's label
base: <full sha — the merge-base pinned in §1>
head: <full sha — the reviewed tip; the note is attached to it>
diff_sha256: <DIFF_SHA>
dimensions: [correctness, reuse, simplification, efficiency, altitude]
findings: { found: <n>, verified: <n>, waived: <n> }
waivers:
  - finding: "<summary>"
    reason: "<reason>"
verdict: PASS   # PASS | FAIL
date: <YYYY-MM-DD>
```

- Attach it: `git notes --ref=cr add -f -m "<yaml>" HEAD_R`.
- Notes travel only when pushed explicitly — remind the caller:
  `git push origin refs/notes/cr`. Audit any tip with
  `git notes --ref=cr show <tip>`.

## 7 · Return to the caller

Report: the verdict; the table of verified findings with severities and
verifier evidence; waivers applied; refuted candidates in one line; the note
ref. On `FAIL`,
state the loop explicitly: the trigger session fixes — a correctness bug gets
its failing, named regression test **first** (per `docs/testing-strategy.html`),
cleanups stay behavior-preserving with the suite green — then re-runs this
skill on the fixed tip until a run returns `PASS`.
