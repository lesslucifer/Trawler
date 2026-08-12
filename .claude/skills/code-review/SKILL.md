---
name: code-review
description: Run Trawler's multi-agent code review — a read-only run over a caller-supplied diff scope; mechanical gate, five isolated finders plus a design finder when the diff carries a design artifact, adversarial verifiers, verdict, and a YAML evidence note on the reviewed tip. Use when a Working Unit or epic completes, before any merge, or to review recent commits. The caller defines the scope (WU branch vs its epic branch, epic vs main, or recent commits) — never assume a base.
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

Run `cargo xtask gate0`. It runs `fmt --check` + `clippy -D warnings` +
`nextest run --all` (fallback `cargo test --all`) and **skips them when the
working tree is byte-identical to the last green run** (cached under
`target/.gate0-green`, a local per-checkout file — never a git note, never
synced). Any tree change — code, test, or whitespace — invalidates the cache
and re-runs the full trio, so a skip can never vouch for a tree it did not
verify. A non-zero exit ⇒ stop, report it to the caller, spawn no agents, write
no evidence.

## 3 · Find — five isolated finders, in parallel (six with §3b)

Spawn five subagents **in a single message** (fresh context each), one per
dimension in `rubric.md`. Each finder prompt contains exactly:

- its dimension name, and the instruction to read
  `.claude/skills/code-review/rubric.md` first — its dimension row, the
  principles, and the exclusions;
- the scope (`BASE..HEAD_R`, plus working tree if in scope) — the finder runs
  `git diff` itself and may read any file in the repo, including the tip's
  `test/v2` and `doc/v1` notes (`git notes --ref=test|doc show HEAD_R`):
  declared waivers and distillation claims are review subjects, not free
  passes;
- the return format: a list of candidate findings, each
  `{location: file:line, claim: <one falsifiable sentence>, severity: blocker|important|nit}`.

## 3b · The design dimension — a sixth finder, when the diff earns it

Run `cargo xtask design-scope <BASE> <HEAD_R>`. It prints the design artifacts
in scope and the rulebook blob to pin — the same function the merge gate uses,
so the two can never disagree about what is design-bearing.

- **No artifact** ⇒ no design finder, and the note carries no `design:` section.
- **One or more** ⇒ spawn the design finder alongside the other five, in the
  same message. Its prompt: the instruction to read
  `.claude/skills/code-review/design-rubric.md` first, the artifact paths the
  command printed, the scope, and the same return format as §3. It is Tier-3 of
  the design flow — soft rules and the new-component warrant, never the hard
  rules Tier-1 already owns.

Its candidates go through §4 verification like any other. The design verdict is
`PASS` when no design finding survives verification unwaived, and it is recorded
whether or not the overall review passes.

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

- `DIFF_SHA=$(git diff --no-color --no-ext-diff BASE..HEAD_R | shasum -a 256 | cut -d' ' -f1)`
  (pipe the working-tree diff in as well when it was in scope). The flags
  match `cargo xtask verify-dod`'s recomputation — the binding must be
  byte-identical.
- The note body is one YAML document, contract `cr/v3`.
  `cargo xtask verify-dod` parses it at every merge gate (see
  `docs/workflow.html`): a Working Unit is not merge-ready and an epic is
  not complete without a `PASS` note whose `diff_sha256` matches the tip
  being merged — so a later rewrite of the branch visibly orphans its
  evidence.

```yaml
schema: cr/v3
scope: "WU capture-pagination -> epic/capture"   # the caller's label
epic: E-CAP                     # identity block — matches the tip's other notes
wu: E-CAP.W2                    # working-unit scope only
branch: wu/capture/pagination   # informative; the ids are the truth
base: <full sha — the merge-base pinned in §1>
head: <full sha — the reviewed tip; the note is attached to it>
diff_sha256: <DIFF_SHA>
dimensions: [correctness, reuse, simplification, efficiency, altitude]
findings: { found: <n>, verified: <n>, waived: <n> }
waivers:
  - finding: "<summary>"
    reason: "<reason>"
design:                         # §3b — present iff design-scope listed artifacts
  artifacts: [docs/work/<the run>.html]   # every path it listed, none omitted
  rulebook: <blob it printed>   # what the design dimension judged against
  findings: { found: <n>, verified: <n>, waived: <n> }
  verdict: PASS                 # PASS | FAIL
verdict: PASS   # PASS | FAIL
date: <YYYY-MM-DD>
```

The `design:` block is what `verify-dod`'s `9·design` check reads: a
design-bearing tip whose note omits it, judges fewer artifacts than the diff
carries, or pins a rulebook the tip does not ship, is refused at the merge. On a
diff with no design artifact the block is absent, and no check asks for it.

- Attach it: `git notes --ref=cr add -f -m "<yaml>" HEAD_R`.
- The pre-push hook syncs `refs/notes/*` to origin automatically. Audit any
  tip with `git notes --ref=cr show <tip>`.

## 7 · Return to the caller

Report: the verdict; the table of verified findings with severities and
verifier evidence; waivers applied; refuted candidates in one line; the note
ref. On `FAIL`,
state the loop explicitly: the trigger session fixes — a correctness bug gets
its failing, named regression test **first** (per `docs/testing-strategy.html`),
cleanups stay behavior-preserving with the suite green — then re-runs this
skill on the fixed tip until a run returns `PASS`.
