//! Tier 4: Real-World Application Workloads Test Runner
//!
//! Executes 5 real-world end-user workflow simulation modules covering:
//! - Marathon continuous playback with sliding window, image LRU, and lyric blur recycling.
//! - Fullscreen DJ immersion workflow with minimal controls, auto-hide, queue reordering, and Kawarp blur.
//! - Multi-language heavy catalog browsing across 10 albums and 5 artists with dynamic i18n.
//! - Cross-platform cold boot, credential vault restore, named pipe deep links, and graceful shutdown.
//! - Active playback continuity combined with cross-page UI navigation and vibrancy lifecycle.

mod common;

#[path = "tier4_workloads/mod.rs"]
mod tier4_workloads;
