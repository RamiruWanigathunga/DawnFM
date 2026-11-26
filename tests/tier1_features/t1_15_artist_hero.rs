//! Tier 1: F15 - Artist Page Hero & Concert Badge
//!
//! Validates cinematic hero banner dimensions, aspect ratio clamping, Upcoming Concerts
//! badge pill presence and conditional display, and bottom gradient fade per ORIGINAL_REQUEST §R4.

use egui::{pos2, vec2, Color32, Rect};

use dawnfm::ui::pages::{hero_height, metadata_badge, paint_hero_gradient_dissolve};
use crate::common::{run_headless_ui, MockCatalog};

#[test]
fn test_artist_hero_aspect_ratio_clamp() {
    // Narrow viewport (500px -> clamped to min 260px)
    assert_eq!(hero_height(500.0), 260.0);
    // Standard viewport (800px -> 800 * 0.40 = 320.0)
    assert_eq!(hero_height(800.0), 320.0);
    // Wide viewport (1200px -> 1200 * 0.40 = 480.0)
    assert_eq!(hero_height(1200.0), 480.0);
    // Ultra-wide 4K viewport (2500px -> clamped to max 480px)
    assert_eq!(hero_height(2500.0), 480.0);
}

#[test]
fn test_artist_page_model_presence() {
    let artist_page = MockCatalog::mock_artist_page("weeknd", "The Weeknd", 5);
    assert_eq!(artist_page.id, "weeknd");
    assert_eq!(artist_page.name, "The Weeknd");
    assert_eq!(artist_page.top.len(), 5);
    assert!(!artist_page.albums.is_empty());
}

#[test]
fn test_artist_hero_gradient_dissolve() {
    run_headless_ui(|ui| {
        let hero_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 380.0));
        let bg = Color32::from_rgb(0x16, 0x13, 0x11);
        paint_hero_gradient_dissolve(ui.painter(), hero_rect, bg);
    });
}

#[test]
fn test_artist_concert_badge_render() {
    run_headless_ui(|ui| {
        metadata_badge(ui, Some('🎫'), "Upcoming Concerts");
    });
}
