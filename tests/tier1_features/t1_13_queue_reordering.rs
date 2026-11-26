//! Tier 1: F13 - Fullscreen Queue Reordering
//!
//! Validates `Queue::move_track` contract, track reordering invariants, index shifting
//! when items move before/after playing track, and out-of-bounds rejection per
//! ORIGINAL_REQUEST §R1 and PROJECT.md § Interface Contracts.

use dawnfm::ui::queue::{Queue, Repeat};

use crate::common::sample_tracks;

#[test]
fn test_move_track_basic_reorder() {
    let mut queue = Queue::new(Repeat::Off);
    queue.replace(sample_tracks(5), 0, None);

    let moved_track_id = queue.tracks[1].id.clone();

    let moved = queue.move_track(1, 3);
    assert!(moved);

    // Track previously at 1 is now at index 3
    assert_eq!(queue.tracks[3].id, moved_track_id);
}

#[test]
fn test_move_track_playing_index_shifts_right() {
    let mut queue = Queue::new(Repeat::Off);
    queue.replace(sample_tracks(5), 2, None); // Track 2 is currently playing (index = 2)

    let current_track_id = queue.current().unwrap().id.clone();

    // Move track 4 (after current) to position 0 (before current)
    let moved = queue.move_track(4, 0);
    assert!(moved);

    // Current playing track should have shifted right from index 2 to index 3
    assert_eq!(queue.index, Some(3));
    assert_eq!(queue.current().unwrap().id, current_track_id);
}

#[test]
fn test_move_track_playing_index_shifts_left() {
    let mut queue = Queue::new(Repeat::Off);
    queue.replace(sample_tracks(5), 2, None); // Track 2 is currently playing (index = 2)

    let current_track_id = queue.current().unwrap().id.clone();

    // Move track 0 (before current) to position 4 (after current)
    let moved = queue.move_track(0, 4);
    assert!(moved);

    // Current playing track should have shifted left from index 2 to index 1
    assert_eq!(queue.index, Some(1));
    assert_eq!(queue.current().unwrap().id, current_track_id);
}

#[test]
fn test_move_track_moving_currently_playing_track() {
    let mut queue = Queue::new(Repeat::Off);
    queue.replace(sample_tracks(5), 1, None); // Track 1 is currently playing (index = 1)

    let current_track_id = queue.current().unwrap().id.clone();

    // Move the active playing track to position 4
    let moved = queue.move_track(1, 4);
    assert!(moved);

    // Playing index must follow the track to position 4
    assert_eq!(queue.index, Some(4));
    assert_eq!(queue.current().unwrap().id, current_track_id);
}

#[test]
fn test_move_track_out_of_bounds_returns_false() {
    let mut queue = Queue::new(Repeat::Off);
    queue.replace(sample_tracks(5), 2, None);

    // Move out-of-bounds from
    assert!(!queue.move_track(10, 1));
    // Move out-of-bounds to
    assert!(!queue.move_track(1, 10));
    // Move to self
    assert!(!queue.move_track(2, 2));

    // State must remain strictly unchanged
    assert_eq!(queue.index, Some(2));
    assert_eq!(queue.tracks.len(), 5);
}
