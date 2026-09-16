---
name: design-review
description: Run Trawler's design dimension as a step of its own — the judgment the code review used to carry, over the design artifacts a diff carries and the rendered files it touches, written as a design/v1 note the merge gate's 9·design check reads. Use when a diff carries a closed design run or a component design, or when it renders a file under ui/src/. A diff that changes no code and carries no design artifact owes nothing here.
---

# Design review — the design dimension, on its own note

This is the step the code review handed over: the same rubric, the same
adversarial verification, the same verdict — on a note of its own, because a
diff can carry a design artifact and change no code at all, and such a diff owes
the verdict without owing a review.

It is **read-only**: no agent here edits the tree or commits, and nothing here
judges code. The code review reads no design artifact, and this step judges no
code path.

## 1 · Scope

Same argument shape as the code review: `<base>..<head>`, or one ref meaning
`<ref>^..<ref>`, and pin `BASE=$(git merge-base <base> <head>)` and
`HEAD_R=$(git rev-parse <head>)`. The caller owns the scope.

## 2 · What the diff owes — `cargo xtask design-scope <BASE> <HEAD_R>`

It prints three things, and it is the same function the merge gate calls, so the
two can never disagree:

- the design **artifacts** in scope, with the rulebook blob to pin;
- the **new surfaces** the diff adds (a `@tier` component, a screen);
- every other **rendered file** it touches (any non-test `.tsx` under
  `ui/src/`).

No artifact, no surface, no rendered file ⇒ the diff owes nothing here: report
that and stop. Nothing is written and no note is asked for.

## 3 · Judge the artifacts, when there are any

Spawn one finder — Tier-3 of the design flow, soft rules and the new-component
warrant, never the hard rules Tier-1 already owns. Its prompt: the instruction to
read `.claude/skills/code-review/design-rubric.md` first, the artifact paths the
command printed, the scope, and the return format `{location, claim, severity}`.

A component design the range retired at bytes the base never held is read at its
last state, which `design-scope` prints (DEC-29, DEC-28).

## 4 · Verify — one fresh verifier per candidate

A verifier receives only the finding and the scope, tries to refute it against
what the design actually says, and resolves uncertainty to false-positive —
`docs/decisions/` first, because the oddity may be load-bearing. Refuted
candidates are dropped and mentioned in one line when reporting back.

## 5 · The verdict

`PASS` when no verified design finding survives unwaived; anything else is
`FAIL`. The verdict is recorded whether or not the code review passed — they are
two steps with two notes.

## 6 · The hand-off accounting

Every **new surface** the diff adds takes a citation — `implements: [{path, blob,
built}]`, the blob being `git rev-parse <HEAD_R>:<the design>`, or the last
commit that held it once this WU retired it (DEC-28). No reason excuses an added
surface.

Every other **rendered file** the diff touches is covered by a citation's `built`
list, or named in `undesigned: [{path, why}]` — a statement of fact about what
this edit did and what made a design unnecessary. "No design" is not a reason;
"a copy fix inside the existing layout" is. If the edit re-shaped a surface and
no design covers it, **FAIL**: the gate cannot see that, only this step can.

## 7 · The note — `design/v1` on `refs/notes/design`

`DESIGN_SHA=$(git diff --no-color --no-ext-diff BASE..HEAD_R -- <the artifacts
and the rendered files design-scope listed> | shasum -a 256 | cut -d' ' -f1)`
— the same flags and the same paths the gate recomputes with, so the binding is
byte-identical.

```yaml
schema: design/v1
epic: E-CAP                     # identity block — matches the tip's other notes
wu: E-CAP.W2                    # working-unit scope only
base: <full sha — the merge-base pinned in §1>
design_sha256: <DESIGN_SHA>     # the artifacts and the rendered files
verdict: PASS                   # PASS | FAIL
artifacts: [docs/work/<the run>.html]   # every artifact design-scope listed, none omitted
rulebook: <the blob it printed> # what this verdict is a claim about
implements:                     # one per design the build read; omit when none was listed
  - path: docs/work/components/<the design>.html
    blob: <git rev-parse HEAD_R:<that path>, or <the last commit that held it>:<that path> once this WU retired it (DEC-28)>
    built: [ui/src/components/<what it produced>.tsx]
undesigned:                     # every other rendered file the diff touched
  - path: ui/src/<the file>.tsx
    why: "<what the edit did, and what made a design unnecessary>"
date: <YYYY-MM-DD>
```

- Attach it: `git notes --ref=design add -f -m "<yaml>" HEAD_R`.
- The pre-push hook syncs `refs/notes/*` to origin automatically.

What `verify-dod` refuses: a design-bearing tip with no `design/v1` note, a note
judging fewer artifacts than the diff carries, a verdict that is not `PASS`, a
rulebook pin the tip does not ship, a `design_sha256` that names other bytes, a
new surface with no citation, a rendered file neither cited nor declared, a
citation naming a file the diff does not render, and a file given both answers.

## 8 · Return to the caller

Report the verdict, the artifacts judged, the verified findings with their
evidence, waivers applied, and the note ref. On `FAIL`, the trigger session
fixes the design or the build and this step re-runs on the fixed tip.
