# Review rubric — what finders look for

Read by every finder before it sweeps the diff. **Floor, not ceiling**: meet
every item below, then judge beyond it — a finding is never invalid merely
because no line here predicted it.

## Dimensions — one finder each

| Dimension | Question it asks | Typical catch in Trawler | Not its job |
|---|---|---|---|
| **Correctness** | What does this change do that its tests don't cover? | Edge cases, races, protocol violations, error paths that panic on hostile input. | Style, taste, formatting. |
| **Reuse** | Does this reimplement something the codebase already has? | Hand-rolled logic where a helper exists — `tests/common/` builders, existing utilities. | Inventing new abstractions to force sharing. |
| **Simplification** | Is there a materially simpler equivalent? | Needless indirection, dead branches, generality with exactly one caller. | Code golf; cleverness that hurts readability. |
| **Efficiency** | Does the hot path pay avoidable costs? | Buffering a streamed body, per-request allocations and copies, unbounded channels or maps. | Micro-optimization outside hot paths. |
| **Altitude** | Is the logic at the right layer? | Protocol logic leaking into the control API; capture policy inside transport code — boundaries per `docs/architecture.html`. | Re-architecting the system. |

## Severity — what the verdict reads

Severity decides the verdict, so it is judged, not labelled. A finder proposes
one; the **verifier's** severity is the one the verdict reads, because only the
verifier read the surrounding code.

| Severity | The test it must pass | Effect |
|---|---|---|
| **blocker** | The code is wrong, unsafe, or breaks a stated contract — a caller or a user gets a wrong answer, a crash, a hang, or a silent loss. | Fails the review. |
| **important** | The code is correct but carries a cost the project has ruled against: a duplicated rule that will drift, a reimplementation of something the tree already owns, an altitude break, a cost that grows with the data. | Fails the review. |
| **nit** | Neither. Nothing observable changes and nothing drifts if it is never fixed — naming, ordering, a clearer local shape. | Does not fail. Recorded and dispositioned in the note. |

**Uncertainty resolves upward, to `important`.** `nit` is the narrow exit, not
the default: a finding is one only after it survives *nothing observable
changes and nothing drifts*. The asymmetry is deliberate and matches the
verifier's resolve-to-false-positive rule — in both, the cheap error is
preferred to the silent one. A nit that fails this test and was called one
anyway is a finding the review let through.

## Principles

| # | Principle | Instances it covers today |
|---|---|---|
| P1 | **Bounded resources.** Nothing accumulates without an explicit bound — data designed to flow is never collected wholesale. | Streamed bodies never buffered outside capture's bounded points; no unbounded queues, maps, or channel backlogs. |
| P2 | **Execution-context discipline.** Code honors the constraints of the context running it; work that would stall that context moves out of it. | No blocking I/O or CPU-heavy work on runtime threads; no lock held across an await; critical sections on shared state stay minimal. |
| P3 | **Sensitive-data containment.** Data with a secrecy or trust boundary never crosses it as a side effect. | CA private-key material and captured payload contents stay out of logs, error strings, and the control API unless redacted. |
| P4 | **Hostile-input resilience.** Anything from outside the process is untrusted wherever it flows; malformed input degrades gracefully — never a panic, never corrupted state. | Proxy-facing parse and I/O paths — network bytes, TLS handshakes, headers — survive garbage. |
| P5 | **Intent legibility.** Names and structure carry intent by themselves; a comment exists only where the *why* cannot live in code (CLAUDE.md rule). | Naming, module shape, and comment discipline across the diff. |

The principles are fixed for now. When rubric distillation is taken up later,
it grows this file by generalizing — a recurring verified finding joins an
existing principle as a new instance (or, rarely, mints a new principle at the
same altitude), and an instance is deleted the moment it can be mechanized
into a lint.

## Exclusions — never report

- Anything Gate 0 already catches: formatting, the clippy policy pinned in
  `Cargo.toml` (`[lints]`) and `clippy.toml`, failing tests. Such a finding is
  itself a process bug.
- Pre-existing issues the reviewed diff neither introduces nor worsens.
- Pure taste with no dimension or principle behind it.
