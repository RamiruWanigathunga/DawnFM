//! Tier 3: Cross-Feature Combinations Test Suite Module Declarations
//!
//! Exposes 5 pairwise and multi-feature interaction test modules covering:
//! - `test_stream_and_image_memory`: F03 (streaming windowing) + F04 (image cache LRU) + F02 (mimalloc)
//! - `test_fullscreen_queue_reorder`: F10 (minimal controls) + F11 (auto-hide) + F12 (fullscreen queue) + F13 (Queue::move_track)
//! - `test_i18n_album_artist_pages`: F08 (i18n) + F09 (UI localization) + F15-F17 (artist page) + F18-F20 (album page)
//! - `test_vault_named_pipe_session`: F24 (Windows vault chunking) + F25 (Named pipe IPC) + F21 (Cargo cfg(windows) isolation)
//! - `test_shader_fallback_autohide`: F14 (Kawarp shader / fallback) + F10 (minimal controls) + F11 (auto-hide) + F06 (macOS vibrancy)

pub mod test_fullscreen_queue_reorder;
pub mod test_i18n_album_artist_pages;
pub mod test_shader_fallback_autohide;
pub mod test_stream_and_image_memory;
pub mod test_vault_named_pipe_session;
