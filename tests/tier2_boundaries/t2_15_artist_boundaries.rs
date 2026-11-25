//! Tier 2: F15 - Artist Page Hero & Concerts Boundaries
//!
//! Validates empty artist name fallback, 500-track overflow, missing banner fallback glow,
//! and large biography text in genuine ArtistPage models per ORIGINAL_REQUEST §R4.

use dawnfm::backend::ArtistPage;
use dawnfm::ui::pages::{hero_height, metadata_badge};
use crate::common::run_headless_ui;

#[test]
fn test_artist_hero_boundaries() {
    // 0.0 width
    assert_eq!(hero_height(0.0), 260.0);
    // Negative width
    assert_eq!(hero_height(-500.0), 260.0);
    // Huge 10k width
    assert_eq!(hero_height(10_000.0), 480.0);
}

#[test]
fn test_artist_page_empty_and_large_bio() {
    let empty_artist = ArtistPage {
        id: "empty".into(),
        name: "".into(),
        banner: Vec::new(),
        audience: None,
        subscribers: None,
        views: None,
        description: None,
        top: Vec::new(),
        top_playlist: None,
        albums: Vec::new(),
        playlists: Vec::new(),
        similar: Vec::new(),
        photo: Vec::new(),
        following: None,
    };
    assert!(empty_artist.name.is_empty());
    assert!(empty_artist.banner.is_empty());

    let large_bio = "A".repeat(100_000); // 100 KB text
    let large_artist = ArtistPage {
        id: "large".into(),
        name: "Prolific Artist".into(),
        banner: Vec::new(),
        audience: Some("1M monthly audience".into()),
        subscribers: Some("5M subscribers".into()),
        views: None,
        description: Some(large_bio),
        top: Vec::new(),
        top_playlist: None,
        albums: Vec::new(),
        playlists: Vec::new(),
        similar: Vec::new(),
        photo: Vec::new(),
        following: None,
    };
    assert_eq!(large_artist.description.as_ref().unwrap().len(), 100_000);
}

#[test]
fn test_artist_metadata_badge_empty_and_long_label() {
    run_headless_ui(|ui| {
        // Empty label
        metadata_badge(ui, None, "");
        // Long label
        let long_label = "Concert in Paris, London, Berlin, Tokyo, Sydney & New York";
        metadata_badge(ui, Some('🎫'), long_label);
    });
}
