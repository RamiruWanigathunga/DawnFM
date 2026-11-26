//! Tier 4: Real-World Application Workloads Test Suite Module Declarations
//!
//! Exposes 5 comprehensive real-world end-user workflow simulation modules covering:
//! - `test_workload_marathon_playback`: 2-hour 40-track continuous playback simulation exercising stream buffer sliding windows (F03), mimalloc heap stability (F02), image cache LRU budget compliance (F04), and lyric blur pool recycling (F05).
//! - `test_workload_fullscreen_dj`: Fullscreen DJ immersion workflow exercising minimal controls (F10), 3.5s auto-hide timer (F11), fullscreen queue panel (F12), queue reordering via `Queue::move_track` (F13), and Kawarp animated blur background (F14).
//! - `test_workload_heavy_browse`: Heavy catalog exploration across 10 albums and 5 artists exercising i18n dynamic translation (F08/F09), hero banner aspect clamping & concert badges (F15), split views & top songs pagination (F17), album ambient glow (F18), action pills & search/sort toolbar (F19), and 7-column tracklists (F20).
//! - `test_workload_lifecycle`: Cold boot, Windows/macOS credential vault secret restore (F24), deep link IPC URL invocation (F25), and graceful application shutdown session.
//! - `test_workload_active_playback_navigation`: Full end-user session combining active playback with UI navigation across multiple views, queue modifications, and vibrancy/styling lifecycle (F06/F07).

pub mod test_workload_active_playback_navigation;
pub mod test_workload_fullscreen_dj;
pub mod test_workload_heavy_browse;
pub mod test_workload_lifecycle;
pub mod test_workload_marathon_playback;
