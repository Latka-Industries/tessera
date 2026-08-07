//! Deterministic golden / sample `.tes` builders.
//!
//! Shared by `examples/gen_v0_fixtures`, `examples/gen_vault_fixtures`,
//! `examples/gen_sample_fixtures`, and `src/tests/golden_v0.rs` so on-disk
//! bytes stay single-sourced for goldens (samples are browse-only).

pub mod packs;
pub mod samples;
pub mod v0;
pub mod vault;
