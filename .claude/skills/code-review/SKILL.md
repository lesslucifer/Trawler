---
name: code-review
description: Run Trawler's multi-agent code review — a read-only run over a caller-supplied diff scope; mechanical preflight, five isolated finders over the diff's code paths, adversarial verifiers, verdict, and a YAML evidence note on the reviewed tip that also records what the round cost. The review judges code and nothing else: no document is its subject, and a diff that changes no code asks for no review at all. Use when a Working Unit or epic completes, before any merge, or to review recent commits. The caller defines the scope (WU branch vs its epic branch, epic vs main, or recent commits) — never assume a base.
---

# Code review — orchestration

A review run is **read-only**: no agent in it edits the tree or commits.
Fixing belongs to the session that triggered the run (§7). What to review
lives in `rubric.md` next to this file — finders read it; this file only
sequences the run.

**The subject is code.** `src/`, `xtask/`, `tests/`, `ui/src/`, `ui/e2e/` —
the paths `notes::is_code` names, one definition shared with the gate. A
document is never judged, cited or bound here: the design dimension left this
run for a step of its own (§3b), and a diff that changes no code path asks for
no review, no note and no finders at all.

## 1 · Resolve scope

- Argument `<base>..<head>` (e.g. `epic/capture..wu-pagination`,
  `main..epic/capture`), or a single ref meaning `<ref>^..<ref>`.
- No argument: `HEAD^..HEAD` plus any uncommitted changes.
- **Never** widen the scope on your own — the caller owns it (usually a WU
  branch against its parent epic branch, or an epic against `main`).
- Pin full SHAs now: `BASE=$(git merge-base <base> <head>)` and
  `HEAD_R=$(git rev-parse <head>)`. Merge-base, not the base tip — commits
  that landed on the base branch after the fork must not pollute the diff.
- The reviewed diff is `git diff BASE..HEAD_R -- <the code paths>`, plus the
  working-tree half when it is in scope. The paths are the five prefixes above;
  read them off `cargo xtask notes write`'s `code_sha256` line when it printed
  one, so the bytes you judge and the bytes you bind are the same ones.
- Optional, repeatable: `--waive "<finding summary>: <reason>"` — the
  trigger's waiver declarations, applied at the verdict (§5).

## 2 · Preflight — abort if red

Run `cargo xtask preflight wu <tip> --target <the epic branch>` (or
`cargo xtask preflight epic <tip>`, against `main`) — the same two levels, and
the same target inference, the merge gate uses. This is the last point at which
a red input is repaired for free: the `cr/v5` note pins the code it judged, so
anything fixed after the verdict buys the fleet again.

It refuses on exactly three things:

- **The tree** — Gate 0: `fmt --check` + `clippy -D warnings` + `nextest run
  --all` (fallback `cargo test --all`), **skipped when the working tree is
  byte-identical to the last green run** (cached under `target/.gate0-green`, a
  local per-checkout file — never a git note, never synced). Any tree change —
  code, test, or whitespace — invalidates the cache and re-runs the full trio,
  so a skip can never vouch for a tree it did not verify.
- **The checkout** — the tree Gate 0 measures has to be the tree being reviewed:
  `HEAD` must be the tip you named, with nothing uncommitted. A green read off
  another commit (or off a working tree that is about to be committed) vouches
  for bytes the verdict is not about, and the refusal says which of the two it
  found.
- **The note this review reads** — `test/v2` on the reviewed tip: present,
  parseable, carrying the identity block, and with the manifest holding the test
  layer this level needs. The same code the merge gate judges it with. `doc/v1`
  is not read here and cannot void this PASS, so the preflight no longer stops on
  it (DEC-43, restated by DEC-45).
- **Nothing else.** No `cr/v5` note (this review writes it), no design section,
  no `mr/v1` (the human signs last), no doc-note coverage, and no doc predicate.

A non-zero exit ⇒ stop, report its blocks to the caller, spawn no agents, write
no evidence. An exempt diff — one that asks for no review at all — needs none:
report that and stop; the preflight measures no tree for a diff that asks for
nothing, so `cargo xtask gate0` is how to ask about the tree on its own.

**WU and epic scopes only.** The preflight protects a verdict the merge gate
binds, and those are the two levels it binds — a WU branch against its epic, or
an epic against `main`. A caller reviewing any other scope (§1 permits recent
commits, a single ref, the working tree) has no note to orphan: run
`cargo xtask gate0` yourself, read the notes you need, and go on. Never point
the preflight at a tip without naming the scope whose notes you actually read.

## 3 · Find — five isolated finders, in parallel

Spawn five subagents **in a single message** (fresh context each), one per
dimension in `rubric.md`. Each finder prompt contains exactly:

- its dimension name, and the instruction to read
  `.claude/skills/code-review/rubric.md` first — its dimension row, the
  principles, and the exclusions;
- the scope (`BASE..HEAD_R`, plus working tree if in scope) — the finder runs
  `git diff` itself and may read any file in the repo, including the tip's
  `test/v2` note (`git notes --ref=test show HEAD_R`): a declared waiver is a
  review subject, not a free pass. `doc/v1` is not read: what a document says
  about the code is the plan's and the human's business, not this run's;
- the return format: a list of candidate findings, each
  `{location: file:line, claim: <one falsifiable sentence>, severity: blocker|important|nit}`
  — the severity rows in `rubric.md` say which is which, and a finder's severity
  is a proposal the verifier re-judges.

## 3b · The design dimension is not part of this run

A diff that carries a design artifact, or renders a file under `ui/src/`, owes
the design dimension's verdict — on the design step's own `design/v1` note, which
`9·design` reads. That step is not this one: this run spawns no design finder,
reads no design artifact, and its note carries no `design:` section, so a
document can neither be judged here nor void anything judged here. Run
`.claude/skills/design-review/SKILL.md` for it, over the same scope.

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
- **judge the severity** against `rubric.md`'s severity rows rather than
  inheriting the finder's: this verdict reads yours, and uncertainty between two
  rows resolves *upward*;
- return `{verdict: real|false-positive, severity, evidence: <why>}`.

Refuted findings are dropped from the verdict; mention them briefly when
returning to the caller (§7) so finder quality stays visible.

## 5 · Verdict

- Match the trigger's `--waive` declarations against verified findings;
  record each waiver with its reason (citing a `docs/decisions/` record where
  one applies).
- **PASS** = Gate 0 green and zero verified **blockers or importants** left
  unwaived. Anything else is **FAIL** — evidence is written either way.
- **A verified nit does not fail the review, and is never silently dropped.**
  Every one goes in the note's `nits:` list as a named finding with **exactly
  one** disposition — `fixed: "<what changed>"` when the fix is already inside the
  range this note binds, or `waived: "<reason>"`. An empty sentence is no
  disposition, an empty `finding` names nothing, and carrying both dispositions
  records a contradiction; the merge gate refuses each of those under
  `3·review-nits` — so the cheap pass buys a
  recorded decision, not an unread list. Nothing here licenses a post-verdict
  fix: a later commit still moves `CODE_SHA` and still orphans this note.

## 6 · Evidence — YAML note on the reviewed tip

- `CODE_SHA=$(git diff --no-color --no-ext-diff BASE..HEAD_R -- <the code paths> | shasum -a 256 | cut -d' ' -f1)`
  (add the working-tree half the same way when it was in scope). The paths are
  the five prefixes from the top of this file; the flags match `cargo xtask
  verify-dod`'s recomputation — the binding must be byte-identical. A diff with
  no code path has no hash to write, and asks for no note at all.
- The note body is one YAML document, contract `cr/v5`.
  `cargo xtask verify-dod` parses it at every merge gate (see
  `docs/workflow.html`): a Working Unit is not merge-ready and an epic is
  not complete without a `PASS` note whose `diff_sha256` matches the code the
  tip changes — so a later code edit visibly orphans its evidence, and a
  document landing after it does not.

```yaml
schema: cr/v5
scope: "WU capture-pagination -> epic/capture"   # the caller's label
epic: E-CAP                     # identity block — matches the tip's other notes
wu: E-CAP.W2                    # working-unit scope only
branch: wu/capture/pagination   # informative; the ids are the truth
base: <full sha — the merge-base pinned in §1>
head: <full sha — the reviewed tip; the note is attached to it>
diff_sha256: <CODE_SHA>         # the code paths, and only them
dimensions: [correctness, reuse, simplification, efficiency, altitude]
fleet:                          # what this round spent; `cargo xtask tally --fleet` reads it back
  finders: 5
  verifiers: <n>                # verifier agents spawned - one per candidate
  by_finder:
    correctness:    { found: <n>, verified: <n> }
    reuse:          { found: <n>, verified: <n> }
    simplification: { found: <n>, verified: <n> }
    efficiency:     { found: <n>, verified: <n> }
    altitude:       { found: <n>, verified: <n> }
findings: { found: <n>, verified: <n>, waived: <n> }   # distinct claims, after dedup
waivers:
  - finding: "<summary>"
    reason: "<reason>"
nits:                           # every verified nit, each one decided
  - finding: "<summary>"
    fixed: "<what changed>"     # the fix is inside the range this note binds
  - finding: "<summary>"
    waived: "<reason>"          # accepted as it stands
verdict: PASS   # PASS | FAIL — nits never decide it
date: <YYYY-MM-DD>
```

The `fleet:` block is the round's own bill, and every note this run writes
carries one. `by_finder` counts what each finder returned and how much of it
the verifiers confirmed, so its sums can exceed `findings` where two finders
raised the same claim — the overlap is a reading, not an error. A note without
the block still parses at the gate; the tally reads it as a round that did not
count itself, never as a round that cost nothing.

There is no `design:` block on this note, and there is nothing to write in its
place: `verify-dod`'s `9·design` check reads the design step's own `design/v1`
note (`.claude/skills/design-review/SKILL.md`), which is what a design-bearing
or surface-rendering diff owes. A note carrying one anyway is a claim this run
cannot make — the verdict would be about documents the binding above does not
cover.

- Attach it: `git notes --ref=cr add -f -m "<yaml>" HEAD_R`.
- The pre-push hook syncs `refs/notes/*` to origin automatically. Audit any
  tip with `git notes --ref=cr show <tip>`.

## 7 · Return to the caller

Report: the verdict; the table of verified findings with severities and
verifier evidence; waivers applied; the nits and how each was dispositioned;
refuted candidates in one line; the fleet's bill as the note records it; the
note ref. On `FAIL`,
state the loop explicitly: the trigger session fixes — a correctness bug gets
its failing, named regression test **first** (per `docs/testing-strategy.html`),
cleanups stay behavior-preserving with the suite green — then re-runs this
skill on the fixed tip until a run returns `PASS`.
