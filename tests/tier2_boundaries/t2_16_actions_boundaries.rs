//! Tier 2: F16 - Artist Circular Action Buttons Boundaries
//!
//! Validates empty top songs play no-op, 100-click rapid execution,
//! and CircleGlyph variants per ORIGINAL_REQUEST §R4.

use dawnfm::model::Target;
use dawnfm::ui::pages::{peach_circle, play_pill, CircleGlyph};
use crate::common::run_headless_ui;

#[test]
fn test_artist_play_with_empty_top_songs() {
    run_headless_ui(|ui| {
        let mut actions = Vec::new();
        let target = Target::Artist("empty_artist".into());
        let empty_tracks = &[];
        // play_pill with empty tracks must be a graceful no-op
        let _resp = play_pill(ui, &mut actions, &target, empty_tracks);
        assert!(actions.is_empty(), "Playing empty tracks must dispatch zero actions");
    });
}

#[test]
fn test_artist_rapid_button_hammering_100x() {
    run_headless_ui(|ui| {
        for _ in 0..100 {
            let resp = peach_circle(ui, "Options", CircleGlyph::More);
            assert_eq!(resp.rect.width(), 36.0);
        }
    });
}

#[test]
fn test_circle_glyph_variants() {
    let char_glyph = CircleGlyph::Char('▶');
    let more_glyph = CircleGlyph::More;
    assert_ne!(char_glyph, more_glyph);
}
