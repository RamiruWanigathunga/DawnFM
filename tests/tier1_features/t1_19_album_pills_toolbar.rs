//! Tier 1: F19 - Album Page Action Pills & Toolbar
//!
//! Validates action pills (Play, Shuffle, + Add) and tracklist real-time search filter
//! per ORIGINAL_REQUEST §R4 and PROJECT.md § Feature 19.

use dawnfm::ui::pages::{filter_collection_tracks, peach_pill};
use crate::common::{run_headless_ui, sample_album_tracks};

#[test]
fn test_album_action_pills_render() {
    run_headless_ui(|ui| {
        let resp_play = peach_pill(ui, "Play", Some('▶'));
        assert!(resp_play.rect.width() > 36.0);

        let resp_shuffle = peach_pill(ui, "Shuffle", Some('🔀'));
        assert!(resp_shuffle.rect.width() > 36.0);

        let resp_add = peach_pill(ui, "Add to Playlist", None);
        assert!(resp_add.rect.width() > 36.0);
    });
}

#[test]
fn test_album_toolbar_filter_search() {
    let tracks = sample_album_tracks();

    let res = filter_collection_tracks(&tracks, "Blinding Lights");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].title, "Blinding Lights");

    let empty = filter_collection_tracks(&tracks, "NonExistentSongXYZ");
    assert!(empty.is_empty());

    let all = filter_collection_tracks(&tracks, "");
    assert_eq!(all.len(), 14);
}

#[test]
fn test_album_toolbar_filter_by_artist() {
    let tracks = sample_album_tracks();
    let res = filter_collection_tracks(&tracks, "The Weeknd");
    assert!(!res.is_empty());
}
