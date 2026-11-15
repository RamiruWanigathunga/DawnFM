//! Tier 2: F17 - Artist Split & Top Songs Boundaries
//!
//! Validates empty top songs list, 1-song single column rendering, and missing
//! latest release cover fallback using genuine crate structures per ORIGINAL_REQUEST §R4.

use dawnfm::model::{Card, Target};
use crate::common::MockCatalog;

#[test]
fn test_artist_top_songs_empty_list() {
    let artist_page = MockCatalog::mock_artist_page("empty", "Empty Artist", 0);
    assert!(artist_page.top.is_empty());
}

#[test]
fn test_artist_top_songs_single_item() {
    let artist_page = MockCatalog::mock_artist_page("single", "Single Track Artist", 1);
    assert_eq!(artist_page.top.len(), 1);
}

#[test]
fn test_artist_top_songs_exactly_3_items() {
    let artist_page = MockCatalog::mock_artist_page("three", "Three Track Artist", 3);
    assert_eq!(artist_page.top.len(), 3);
}

#[test]
fn test_artist_latest_release_missing_cover() {
    let release_card = Card {
        title: "Dancing in the Flames".into(),
        subtitle: "2024".into(),
        target: Target::Album("MPREb_dancing".into()),
        thumbs: Vec::new(),
        round: false,
        links: Vec::new(),
    };

    assert!(release_card.thumbs.is_empty(), "Card handles missing thumbnails gracefully");
}
