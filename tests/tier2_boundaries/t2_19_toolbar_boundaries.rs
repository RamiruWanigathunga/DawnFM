//! Tier 2: F19 - Album Action Pills & Toolbar Boundaries
//!
//! Validates regex injection resilience in search, emoji needle search, and
//! large tracklist filtering using genuine crate logic per ORIGINAL_REQUEST §R4.

use dawnfm::ui::pages::filter_collection_tracks;
use crate::common::sample_tracks;

#[test]
fn test_album_toolbar_regex_injection_search() {
    let tracks = sample_tracks(5);
    let dangerous_needles = [
        ".*",
        "[a-z]+",
        "(.*?)",
        "\\d{4}",
        "^.*$",
        "???",
        "+++",
    ];

    for needle in dangerous_needles {
        // Plain substring search must never crash or panic on regex meta-characters
        let res = filter_collection_tracks(&tracks, needle);
        assert!(res.len() <= tracks.len());
    }
}

#[test]
fn test_album_toolbar_emoji_search_needle() {
    let mut tracks = sample_tracks(3);
    tracks[0].title = "Sparkle 🎵 Star".to_string();
    tracks[1].title = "Night Drive 🌙".to_string();

    let emoji_needle = "🎵";
    let res = filter_collection_tracks(&tracks, emoji_needle);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, tracks[0].id);

    let moon_needle = "🌙";
    let res2 = filter_collection_tracks(&tracks, moon_needle);
    assert_eq!(res2.len(), 1);
    assert_eq!(res2[0].id, tracks[1].id);
}

#[test]
fn test_album_toolbar_large_needle_and_empty_list() {
    let empty_tracks = [];
    let res = filter_collection_tracks(&empty_tracks, "query");
    assert!(res.is_empty());

    let long_needle = "a".repeat(10_000);
    let tracks = sample_tracks(10);
    let res_long = filter_collection_tracks(&tracks, &long_needle);
    assert!(res_long.is_empty());
}
