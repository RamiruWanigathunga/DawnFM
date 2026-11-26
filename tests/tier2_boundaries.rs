//! Tier 2: Boundary & Corner Cases Test Runner
//!
//! Executes all 26 boundary modules (t2_01 through t2_26), verifying at least 130
//! boundary, extreme, and malformed input test cases against Dawn FM contracts headlessly.

mod common;

#[path = "tier2_boundaries/mod.rs"]
mod tier2_boundaries;
