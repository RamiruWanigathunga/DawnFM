//! Tier 2: F13 - Fullscreen Queue Reordering Boundaries
//!
//! Validates identity moves (from == to), 1-item queue moves, 2-item bidirectional swaps,
//! first-to-last boundary moves, last-to-first boundary moves, and out-of-bounds rejection per ORIGINAL_REQUEST §R1.

use dawnfm::ui::queue::{Queue, Repeat};

use crate::common::sample_tracks;

#[test]
fn test_move_track_to_self_identity() {
    let mut q = Queue::new(Repeat::Off);
    q.adopt(sample_tracks(5), 2);
    let original_id = q.tracks[2].id.clone();

    // Move to self
    let moved = q.move_track(2, 2);
    assert!(!moved, "Moving track to same index must return false (no-op)");
    assert_eq!(q.index, Some(2));
    assert_eq!(q.tracks[2].id, original_id);
}

#[test]
fn test_move_track_single_item_queue() {
    let mut q = Queue::new(Repeat::Off);
    q.adopt(sample_tracks(1), 0);

    let moved_self = q.move_track(0, 0);
    assert!(!moved_self);

    let moved_oob = q.move_track(0, 1);
    assert!(!moved_oob, "Out-of-bounds destination must return false");
    assert_eq!(q.index, Some(0));
    assert_eq!(q.tracks.len(), 1);
}

#[test]
fn test_move_track_two_items_swap_back_forth() {
    let mut q = Queue::new(Repeat::Off);
    q.adopt(sample_tracks(2), 0);
    let id0 = q.tracks[0].id.clone();
    let id1 = q.tracks[1].id.clone();

    // 1. Move playing track 0 to 1
    assert!(q.move_track(0, 1));
    assert_eq!(q.tracks[0].id, id1);
    assert_eq!(q.tracks[1].id, id0);
    assert_eq!(q.index, Some(1), "Playing index must shift to 1");

    // 2. Move playing track 1 back to 0
    assert!(q.move_track(1, 0));
    assert_eq!(q.tracks[0].id, id0);
    assert_eq!(q.tracks[1].id, id1);
    assert_eq!(q.index, Some(0), "Playing index must shift back to 0");
}

#[test]
fn test_move_track_first_to_last_boundary() {
    let tracks = sample_tracks(6);
    let first_id = tracks[0].id.clone();
    let second_id = tracks[1].id.clone();

    // Active track is at index 3
    let mut q = Queue::new(Repeat::Off);
    q.adopt(tracks, 3);
    let playing_id = q.tracks[3].id.clone();

    // Move first item (0) to last (5)
    let moved = q.move_track(0, 5);
    assert!(moved);

    // Track 1 is now at index 0
    assert_eq!(q.tracks[0].id, second_id);
    // Original track 0 is now at index 5
    assert_eq!(q.tracks[5].id, first_id);
    // Since item moved from before (0) to after (5) playing track (3),
    // playing track shifted from 3 down to 2
    assert_eq!(q.index, Some(2));
    assert_eq!(q.tracks[2].id, playing_id);
}

#[test]
fn test_move_track_last_to_first_boundary() {
    let tracks = sample_tracks(6);
    let last_id = tracks[5].id.clone();

    // Active track is at index 2
    let mut q = Queue::new(Repeat::Off);
    q.adopt(tracks, 2);
    let playing_id = q.tracks[2].id.clone();

    // Move last item (5) to first (0)
    let moved = q.move_track(5, 0);
    assert!(moved);

    // Former last item is now at index 0
    assert_eq!(q.tracks[0].id, last_id);
    // Since item moved from after (5) to before (0) playing track (2),
    // playing track shifted from 2 up to 3
    assert_eq!(q.index, Some(3));
    assert_eq!(q.tracks[3].id, playing_id);

    // Negative tests: out-of-bounds
    assert!(!q.move_track(0, 6));
    assert!(!q.move_track(6, 0));
    assert!(!q.move_track(100, 200));
}
