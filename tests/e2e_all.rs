//! Unified E2E Test Suite Master Runner
//!
//! Executes the complete 4-tier testing pyramid for Dawn FM (290 tests total):
//! - Tier 1: Functional Feature Coverage (26 modules, 130 tests)
//! - Tier 2: Boundary & Corner Cases (26 modules, 130 tests)
//! - Tier 3: Cross-Feature Combinations (5 modules, 25 tests)
//! - Tier 4: Real-World Application Workloads (5 modules, 25 tests)
//!
//! All tests run 100% headlessly in software without physical audio hardware,
//! GPU devices, or active window servers.

mod common;

#[path = "tier1_features/mod.rs"]
mod tier1_features;

#[path = "tier2_boundaries/mod.rs"]
mod tier2_boundaries;

#[path = "tier3_combinations/mod.rs"]
mod tier3_combinations;

#[path = "tier4_workloads/mod.rs"]
mod tier4_workloads;
