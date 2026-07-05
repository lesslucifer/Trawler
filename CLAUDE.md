## Project

Trawler — native MITM HTTP(S) debugging proxy, API-first core.

**Stack**: Rust 2021 · tokio · hudsucker 0.24 (MITM proxy, `:3000`) · axum 0.8
(control API: REST + SSE + `/ca.crt`, `:3001`) · rcgen 0.14 (CA). Lints:
clippy pedantic, `unsafe_code` forbidden (see `Cargo.toml`).

**Layout**: lib crate `src/lib.rs` (`run(Config)` composition root) + thin bin
`src/main.rs`. Modules: `config` · `flow` (domain model) · `event` · `capture`
· `store` (ring + bus) · `recorder` (proxy handler) · `ca` · `api/` (routes,
views, decode). Integration/e2e tests in `tests/` (`test-utils` feature =
shared test support). Generated CA in `ca/`, git-ignored.

**Commands**: `cargo run` (`RUST_LOG=trawler=debug` for logs) · `cargo test` ·
`cargo clippy`. Quick check: `curl -x http://127.0.0.1:3000 http://example.com/`
→ `curl http://127.0.0.1:3001/flows`.

Architecture & invariants: `docs/core-architecture.html` · rationale:
`docs/core-technical-design.html`.

## AI-first (IMPORTANT — direction for ALL work)

All code is written by AI agents; no human reads diffs before they land.
Design everything — docs, plans, processes — for AI readers/executors. Never
produce work estimates (effort, timelines, sizing).

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

**Decide a doc's tier BEFORE creating it and put it in that tier's folder —
never default into `docs/` (SUPER CRITICAL). T1 = permanent AND shape-level; any
plan/spec/test-plan/backlog fails that and is T3 (`docs/work/`), never T1.**

Spec: `docs/documentation-system.html`. **T0** this file (user-edited only) ·
**T1** `docs/*.html` architecture + process · **T2** `docs/use-cases/` (what) &
`docs/decisions/` (why), read index-first · **T3** `docs/work/` scratch, distill
then archive · **T4** `src/` — the truth. One fact per tier (else link); code
wins conflicts; never copy code into docs.

Style: **T0** super compact, ≤200 lines · **T1** concise & direct — no repeats,
no extra detail, no out-of-scope explanation.

**Done = distilled**: a task ends only when the distillation checklist has run
and the affected T2 docs + index (rarely T1) reflect the new truth — same
session as the change, never "later".
