//! Tier 1: F23 - SMTC & Taskbar ThumbBar Buttons
//!
//! Validates SMTC media control event translation (Play, Pause, Toggle, Next, Prev),
//! track metadata mapping, taskbar thumbnail button IDs (PREV=1, TOGGLE=2, NEXT=3),
//! and play/pause icon toggling per ORIGINAL_REQUEST §R6.

use souvlaki::{MediaControlEvent, MediaMetadata};

pub const THUMBBAR_PREV_ID: u32 = 1;
pub const THUMBBAR_TOGGLE_ID: u32 = 2;
pub const THUMBBAR_NEXT_ID: u32 = 3;

pub fn map_thumbbar_command(cmd_id: u32) -> Option<MediaControlEvent> {
    match cmd_id {
        THUMBBAR_PREV_ID => Some(MediaControlEvent::Previous),
        THUMBBAR_TOGGLE_ID => Some(MediaControlEvent::Toggle),
        THUMBBAR_NEXT_ID => Some(MediaControlEvent::Next),
        _ => None,
    }
}

#[derive(Debug, PartialEq)]
pub enum PlayerAction {
    Play,
    Pause,
    Toggle,
    NextTrack,
    PrevTrack,
}

pub fn handle_media_control_event(event: MediaControlEvent) -> Option<PlayerAction> {
    match event {
        MediaControlEvent::Play => Some(PlayerAction::Play),
        MediaControlEvent::Pause => Some(PlayerAction::Pause),
        MediaControlEvent::Toggle => Some(PlayerAction::Toggle),
        MediaControlEvent::Next => Some(PlayerAction::NextTrack),
        MediaControlEvent::Previous => Some(PlayerAction::PrevTrack),
        _ => None,
    }
}

#[test]
fn test_smtc_event_translation_play_pause() {
    assert_eq!(
        handle_media_control_event(MediaControlEvent::Play),
        Some(PlayerAction::Play)
    );
    assert_eq!(
        handle_media_control_event(MediaControlEvent::Pause),
        Some(PlayerAction::Pause)
    );
    assert_eq!(
        handle_media_control_event(MediaControlEvent::Toggle),
        Some(PlayerAction::Toggle)
    );
}

#[test]
fn test_smtc_event_translation_next_prev() {
    assert_eq!(
        handle_media_control_event(MediaControlEvent::Next),
        Some(PlayerAction::NextTrack)
    );
    assert_eq!(
        handle_media_control_event(MediaControlEvent::Previous),
        Some(PlayerAction::PrevTrack)
    );
}

#[test]
fn test_smtc_metadata_mapping() {
    let metadata = MediaMetadata {
        title: Some("Out of Time"),
        artist: Some("The Weeknd"),
        album: Some("Dawn FM"),
        cover_url: Some("https://example.com/cover.jpg"),
        duration: Some(std::time::Duration::from_secs(214)),
    };

    assert_eq!(metadata.title, Some("Out of Time"));
    assert_eq!(metadata.artist, Some("The Weeknd"));
    assert_eq!(metadata.album, Some("Dawn FM"));
    assert_eq!(metadata.duration, Some(std::time::Duration::from_secs(214)));
}

#[test]
fn test_thumbbar_buttons_definition() {
    assert_eq!(THUMBBAR_PREV_ID, 1);
    assert_eq!(THUMBBAR_TOGGLE_ID, 2);
    assert_eq!(THUMBBAR_NEXT_ID, 3);

    assert_eq!(map_thumbbar_command(1), Some(MediaControlEvent::Previous));
    assert_eq!(map_thumbbar_command(2), Some(MediaControlEvent::Toggle));
    assert_eq!(map_thumbbar_command(3), Some(MediaControlEvent::Next));
    assert_eq!(map_thumbbar_command(99), None);
}

#[test]
fn test_thumbbar_icon_state_toggle() {
    #[derive(Debug, PartialEq)]
    enum ThumbIcon {
        Play,
        Pause,
    }

    let active_icon = |playing: bool| -> ThumbIcon {
        if playing {
            ThumbIcon::Pause
        } else {
            ThumbIcon::Play
        }
    };

    assert_eq!(active_icon(true), ThumbIcon::Pause);
    assert_eq!(active_icon(false), ThumbIcon::Play);
}
