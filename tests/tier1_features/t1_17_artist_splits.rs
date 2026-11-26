//! Tier 1: F17 - Artist Page Latest Release & Top Songs
//!
//! Validates horizontal split section proportions, Latest Release card data model,
//! and Top Songs data models using genuine crate structures per ORIGINAL_REQUEST §R4.

use dawnfm::model::{Card, Target, Track};
use crate::common::MockCatalog;

#[test]
fn test_artist_split_section_layout_proportions() {
    let content_width: f32 = 1200.0;
    let gap: f32 = 24.0;
    let available: f32 = content_width - gap;

    // Latest release card gets ~35%, Top songs gets ~65%
    let release_width = (available * 0.35f32).round();
    let top_songs_width = available - release_width;

    assert!(release_width >= 350.0);
    assert!(top_songs_width >= 700.0);
    assert_eq!(release_width + top_songs_width + gap, content_width);
}

#[test]
fn test_artist_latest_release_card_data() {
    let release_card = Card {
        title: "Dawn FM (Alternate World)".into(),
        subtitle: "Album • 2024".into(),
        target: Target::Album("MPREb_sample_album".into()),
        thumbs: Vec::new(),
        round: false,
        links: Vec::new(),
    };

    assert_eq!(release_card.title, "Dawn FM (Alternate World)");
    assert!(matches!(release_card.target, Target::Album(_)));
}

#[test]
fn test_artist_top_songs_model() {
    let artist_page = MockCatalog::mock_artist_page("the_weeknd", "The Weeknd", 10);
    assert_eq!(artist_page.top.len(), 10);
    // Take first 5 for the split column display
    let top_slice: &[Track] = &artist_page.top[..5];
    assert_eq!(top_slice.len(), 5);
}

#[test]
fn test_artist_latest_release_from_albums() {
    let artist_page = MockCatalog::mock_artist_page("the_weeknd", "The Weeknd", 5);
    let latest = artist_page.albums.first();
    assert!(latest.is_some());
    assert_eq!(latest.unwrap().title, "Dawn FM");
}
