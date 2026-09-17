---
name: design-review
description: Run Trawler's design step — the hand-off accounting over the design artifacts a diff carries and the rendered files it touches, written as a design/v1 note the merge gate's 9·design check reads. Use when a diff carries a closed design run or a component design, or when it renders a file under ui/src/. A diff that changes no code and carries no design artifact owes nothing here.
---

# Design review — the hand-off accounting, on its own note

This is what is left of the step the code review handed over. DEC-53 dropped the
adversarial design finder: the human reviewing the mockup is the gate, and Tier-1
owns the hard rules. What remains is the accounting — which design each new
surface was built from, and what made a design unnecessary for every other file
the diff renders — on a note of its own, because a diff can carry a design
artifact and change no code at all.

It is **read-only**: no agent here edits the tree or commits, and nothing here
judges code. This step spawns nothing.

## 1 · Scope

Same argument shape as the code review: `<base>..<head>`, or one ref meaning
`<ref>^..<ref>`, and pin `BASE=$(git merge-base <base> <head>)` and
`HEAD_R=$(git rev-parse <head>)`. The caller owns the scope.

## 2 · What the diff owes — `cargo xtask design-scope <BASE> <HEAD_R>`

It prints three things, and it is the same function the merge gate calls, so the
two can never disagree:

- the design **artifacts** in scope;
- the **new surfaces** the diff adds (a `@tier` component, a screen);
- every other **rendered file** it touches (any non-test `.tsx` under
  `ui/src/`).

No artifact, no surface, no rendered file ⇒ the diff owes nothing here: report
that and stop. Nothing is written and no note is asked for.

## 3 · The hand-off accounting

Every **new surface** the diff adds takes a citation — `implements: [{path, blob,
built}]`, the blob being `git rev-parse <HEAD_R>:<the design>`, or the last
commit that held it once this WU retired it (DEC-28). No reason excuses an added
surface.

Every other **rendered file** the diff touches is covered by a citation's `built`
list, or named in `undesigned: [{path, why}]` — a statement of fact about what
this edit did and what made a design unnecessary. "No design" is not a reason;
"a copy fix inside the existing layout" is. If the edit re-shaped a surface and
no design covers it, **FAIL**: the gate cannot see that, only this step can.

## 4 · The verdict

`PASS` when the accounting above is complete: every new surface cited, every
other rendered file cited or declared. Anything else is `FAIL`. The verdict is
recorded whether or not the code review passed — they are two steps with two
notes, and nothing here is a judgment of the design itself (DEC-53).

## 5 · The note — `design/v1` on `refs/notes/design`

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
naming fewer artifacts than the diff carries, a verdict that is not `PASS`, a
`design_sha256` that names other bytes, a new surface with no citation, a
rendered file neither cited nor declared, a citation naming a file the diff does
not render, and a file given both answers.

## 6 · Return to the caller

Report the verdict, the artifacts in scope, the citations written, the files
declared undesigned with their reasons, and the note ref. On `FAIL`, the trigger
session fixes the accounting or the build and this step re-runs on the fixed
tip.
