## Coding conventions (IMPORTANT — apply to all code, incl. tests)

- **Write self-documenting code. Avoid comments unless essential for complex
  logic or critical context** (SUPER CRITICAL — ALWAYS FOLLOW THIS). Names and
  structure carry intent; a comment earns its place only when the _why_ can't be
  expressed in code (a non-obvious invariant, a concurrency race, a guardrail
  rationale). Domain/use-case rationale lives in the HTML design docs, not code.

## Testing (mandatory — no change lands without its test)

Spec: `docs/testing-strategy.html`. Work maps to a test level, each
non-negotiable and additive (they compose, never substitute): **Epic → e2e**
(full flow) · **Working Unit → integration** (its bounded contract) · **Commit →
unit** (all code it adds/changes). Tests are deterministic and hermetic
(ephemeral ports, tempfiles, local servers — never the public network).

## Documentation rules

- **All project docs are interactive HTML in `docs/`, never markdown** (CLAUDE.md
  and skill files are infrastructure, they stay markdown). Authoring template,
  shared assets, dark-mode style, React widgets, the comment layer, and the
  snapshot/diff review flow are all owned by the global **`htmldoc` skill** — follow it; don't reinvent any of it (SUPER CRITICAL — ALWAYS FOLLOW THIS).
  **Snapshot a doc before editing it** and after a round share `<doc>.html?diff=latest`.

## Documentation tiers — synthesize UP, never duplicate DOWN

Spec: `docs/documentation-system.html`. **T0** this file (user-edited only) ·
**T1** `docs/*.html` architecture + process · **T2** `docs/use-cases/` (what) &
`docs/decisions/` (why), read index-first · **T3** `docs/work/` scratch, distill
then archive · **T4** `src/` — the truth. One fact per tier (else link); code
wins conflicts; never copy code into docs; distill before task ends.
