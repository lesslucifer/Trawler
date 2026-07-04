## Coding conventions (IMPORTANT — apply to all code, incl. tests)

- **Write self-documenting code. Avoid comments unless essential for complex
  logic or critical context** (SUPER CRITICAL — ALWAYS FOLLOW THIS). Names and
  structure carry intent; a comment earns its place only when the _why_ can't be
  expressed in code (a non-obvious invariant, a concurrency race, a guardrail
  rationale). Domain/use-case rationale lives in the HTML design docs, not code.

## Documentation rules

- **All project docs are interactive HTML in `docs/`, never markdown** (CLAUDE.md
  and skill files are infrastructure, they stay markdown). Authoring template,
  shared assets, dark-mode style, React widgets, the comment layer, and the
  snapshot/diff review flow are all owned by the global **`htmldoc` skill** — follow it; don't reinvent any of it (SUPER CRITICAL — ALWAYS FOLLOW THIS).
- **At the start of any docs task**, read the comment file of each doc you'll
  touch and address `"status": "open"` entries first (mark them resolved + reply).
  **Snapshot a doc before editing it** and after a round share `<doc>.html?diff=latest`.

## Documentation tiers — synthesize UP, never duplicate DOWN

Spec: `docs/documentation-system.html`. **T0** this file (user-edited only) ·
**T1** `docs/*.html` permanent spine · **T2** `docs/use-cases/` (what) &
`docs/decisions/` (why), indexed, read index-first · **T3** `docs/work/`
scratch, distill then archive · **T4** `src/` — the truth. One fact, one tier;
on conflict code wins; distill before a task ends.
