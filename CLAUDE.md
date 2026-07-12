## Project

Trawler — native MITM HTTP(S) debugging proxy, API-first core.

**Stack**: Rust 2021 · tokio · hudsucker 0.24 (MITM proxy, `:7420`) · axum 0.8
(control API: REST + SSE + `/ca.crt`, `:7421`) · rcgen 0.14 (CA). Native UI:
Tauri 2 + Vite + TypeScript + React + Tailwind in `ui/` (out-of-process API
client; mirrors Strata). Lints: clippy pedantic, `unsafe_code` forbidden
(see `Cargo.toml`).

**Layout**: lib crate `src/lib.rs` (`run(Config)` composition root) + thin bin
`src/main.rs`. Modules: `config` · `flow` (domain model) · `event` · `capture`
· `store` (ring + bus) · `recorder` (proxy handler) · `ca` · `api/` (routes,
views, decode). Integration/e2e tests in `tests/` (`test-utils` feature =
shared test support). Native app in `ui/` (`ui/src-tauri` a standalone crate
outside the core workspace — headless `cargo build`/`test` stays GUI-free).
Generated CA in `ca/`, git-ignored.

**Commands**: `cargo run` (`RUST_LOG=trawler=debug` for logs) · `cargo test` ·
`cargo clippy` · `cargo xtask ui build|dev|e2e` (front-end bundle · Tauri
window · WebDriver e2e). Quick check: `curl -x http://127.0.0.1:7420 http://example.com/`
→ `curl http://127.0.0.1:7421/flows`.

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

## Workflow (mandatory — how work lands)

Spec: `docs/workflow.html`. **Epic → Working Unit → commit**; branches
`epic/<slug>` off `main`, `wu/<epic-slug>/<slug>` off its epic; git is the
registry, no tracker file. **Ids**: epic = `E-<CODE>` (uppercase slug abbrev,
e.g. `E-INFRA`, `E-UI`); WU = numeric within its epic, `E-<CODE>.W<m>`
(e.g. `E-INFRA.W1`, `E-UI.W5`). Plan before code: the epic's
T3 plan lists each WU's requirement sentence + test plan. Small epic = one WU,
small WU = one commit — every gate still met explicitly.

**The merge is the gate**: always `--no-ff`; hooks run `cargo xtask verify-dod`
and a red gate refuses the merge — never bypass (`--no-verify`/ff only defers
the red: the epic gate re-walks every seam). Before merging a tip: `test/v2` +
`doc/v1` notes on it, then the **code-review skill** → `cr/v2` PASS bound to
that exact diff (never judge your own diff). Green writes the `dod/v1` receipt.
Never amend/rebase a gated branch — it orphans the notes. Skipping a test
layer = `waived: "<why>"` in the note; docs/infra-only diffs are exempt.
Standalone code commits outside a WU carry `test/v2` with `level: commit`.

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

Spec: `docs/documentation-system.html`. **T0** this file (edited only on
explicit request — never silently or as a side effect) ·
**T1** `docs/*.html` architecture + process · **T2** `docs/use-cases/` (what) &
`docs/decisions/` (why), read index-first · **T3** `docs/work/` scratch, distill
then archive · **T4** `src/` — the truth. One fact per tier (else link); code
wins conflicts; never copy code into docs.

Style: **T0** super compact, ≤200 lines · **T1** concise & direct — no repeats,
no extra detail, no out-of-scope explanation.

**Done = distilled**: a task ends only when the distillation checklist has run
and the affected T2 docs + index (rarely T1) reflect the new truth — same
session as the change, never "later".
