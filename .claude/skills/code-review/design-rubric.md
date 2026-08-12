# Design rubric — what the design finder looks for

Read by the **design finder** only, and only when the diff carries a design
artifact (§1). It is Tier-3 of the design flow: the independent judgment of a
design the author already judged themselves. `rubric.md` governs the other five
dimensions; nothing here applies to built code.

## 1 · Subject — what activates this finder

`cargo xtask design-scope <base> <head>` answers it, and its answer is the one
the merge gate enforces. Two kinds:

| Artifact | Is judged when |
|---|---|
| **Design run** — a doc declaring `data-artifact="design-run"` | it is **closed** at the head (`data-status="archived"`). A `draft` run is unfinished; it is judged by the review of the merge that closes it. |
| **Component design file** — `docs/work/components/<slug>.html` | it is present at the head. A *removed* one is retirement, not design (`6·retirement-order` owns it). |

Judge **every** artifact the command lists. The note's `design.artifacts` must
name each one, so an unjudged artifact is a red merge, not a quiet omission.

## 2 · Scope — soft rules and the new-component warrant, nothing else

The substrate, read at the head being reviewed — never from memory:

- **the rulebook**: `docs/design-rules/design-rules-index.html`, its
  **soft-rules table**. `git hash-object` it: that blob is the note's
  `design.rulebook` pin, and it is what your verdict is a claim about.
- **the registry**: `node scripts/design-registry.mjs` — every component's
  tier, `@use`, `@avoid`, `@states`, `@madeof`, from the collection's own TSDoc.
- the run's **receipt** (`data-item="receipt"`), its **elements** catalog, and
  its **proposals** list.

| # | What you judge | The question |
|---|---|---|
| D1 | **Every soft-rule verdict in the receipt** | Does the artifact *earn* the verdict it claims? A fluent `satisfied` against a design that does not satisfy it is this tier's highest-value catch — the author wrote both the design and the claim. |
| D2 | **Soft rules against the design itself** | Read the Frame goals, the Layout, the mockup. Does the hierarchy match the goal ranking, the grouping match semantics, the order match scan priority — regardless of what the receipt says? A rule with a *true* `satisfied` row can still be broken by the artifact the row does not describe. |
| D3 | **The new-component warrant** | Every `propose`/`extend` row: does an existing entry's `@use` already cover it? Does the cited `@avoid` really exclude the reuse? A component invented while one fit is the drift the registry exists to stop, and the citation being *well-formed* is what makes it invisible. |
| D4 | **The reuse warrant, the same question inverted** | Every `reuse` row: does the component it names actually fit this use, or does its `@avoid` exclude it? Tier-1 checks the citation resolves; only a reader checks it is *true*. |
| D5 | **`blocked` / `n/a` / `inherited` verdicts** | `n/a` on a rule that does apply, and `blocked` on something the design could have decided, are how a design exits a rule without satisfying it. `inherited` is proven by its citation — follow it to the component's TSDoc. |

## 3 · Exclusions — never report

- **Hard rules.** All thirteen have Tier-1 predicates (`cargo xtask verify-doc`
  is green before you are called). A second opinion on them is noise, and a
  finding against one is a process bug — file it as a Tier-1 gap instead.
- **A per-rule verdict table.** That is Tier-2's receipt; re-emitting it doubles
  the artifact and hides your findings inside it. Report findings, or PASS.
- **Taste with no slug.** Every finding cites a soft-rule slug, a component's
  `@use`/`@avoid`, or a decision record. A judgment that cites nothing is a
  *rulebook hole* — report it as one, naming what it would take to rule.
- **Rulebook edits.** If a soft rule cannot be judged as written, say so as a
  finding. Loosening a rule to fit the artifact that tripped it is the failure
  DEC-15 names; the rule change is its own work with its own reasoning.
- **Built code**, doc formatting, and anything `rubric.md`'s five dimensions or
  Gate 0 already own.

## 4 · Calibration — the bar this finder is held to

These three are documented defects in archived runs. A design finder that
passes an artifact carrying one of them is not ready to judge a live design;
they are the shape of what this tier exists to catch.

| Defect | Where | Why nothing else caught it |
|---|---|---|
| A **wrong reuse, correctly cited** — `CopyButton`'s `@use` quoted accurately while both components' `@avoid` lines excluded the reuse | run #3, finding 21 | Tier-1 checked the citation resolved; the receipt was the author's own. A whole stage shipped wrong. |
| A **false `satisfied`** — the row read fluently and the design did not satisfy the rule | run #2 (`spacing-scale`) | Only composing the real CSS disproved it. The prose was the evidence, and the author wrote the prose. |
| A **capability designed away**, then reported as blocked | run #3 (`proxy unset`) | The design was internally consistent; only the cross-check against shipped code disagreed. |

## 5 · Findings

Same format as every other finder — `{location: file:line, claim: <one
falsifiable sentence>, severity: blocker|important|nit}` — with the slug in the
claim. A verifier will try to refute you against the artifact and the substrate,
so a claim that cannot be checked against either will not survive. Uncertainty
resolves to no-finding: the design was already gated by thirteen predicates and
a forced-coverage receipt, and a false design finding costs a real revision.
