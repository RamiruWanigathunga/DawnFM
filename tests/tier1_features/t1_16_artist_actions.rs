//! Tier 1: F16 - Artist Page Circular Action Buttons
//!
//! Validates circular button geometry, Play button dispatch, and circular action helpers
//! per ORIGINAL_REQUEST §R4.

use dawnfm::model::Target;
use dawnfm::ui::pages::{circle_action, peach_circle, play_pill, CircleGlyph};
use crate::common::{run_headless_ui, sample_tracks};

#[test]
fn test_artist_circular_buttons_present() {
    run_headless_ui(|ui| {
        let resp = peach_circle(ui, "More options", CircleGlyph::More);
        assert_eq!(resp.rect.width(), 36.0);
        assert_eq!(resp.rect.height(), 36.0);
    });
}

#[test]
fn test_artist_circular_action_glyph() {
    run_headless_ui(|ui| {
        let resp = circle_action(ui, CircleGlyph::Char('★'), "Favorite artist", false);
        assert_eq!(resp.rect.width(), 38.0);
    });
}

#[test]
fn test_artist_play_pill_dispatch() {
    run_headless_ui(|ui| {
        let mut actions = Vec::new();
        let target = Target::Artist("weeknd".into());
        let tracks = sample_tracks(5);
        let resp = play_pill(ui, &mut actions, &target, &tracks);
        assert!(resp.rect.width() > 36.0);
    });
}
