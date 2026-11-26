//! Tier 3: Cross-Feature Combinations Test Runner
//!
//! Executes 5 cross-feature interaction modules covering pairwise and multi-feature
//! subsystem interactions under concurrent streaming, UI state transitions,
//! dynamic locale switching, Windows IPC session restore, and visual shader compositing.

mod common;

#[path = "tier3_combinations/mod.rs"]
mod tier3_combinations;
