#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

mod ci_workflow;
mod doc_examples;
mod doc_hooks;
mod e2e_gate_parity;
mod e2e_merge_proof;
mod gate0_cache;
mod harness_parity;
mod hermetic;
mod hooks_install;
mod lint_spawn;
mod mr;
mod notes_write;
mod preflight;
mod release_workflow;
mod tally;
mod target_isolation;
mod ui;
mod verify_doc;
mod walkthrough;
