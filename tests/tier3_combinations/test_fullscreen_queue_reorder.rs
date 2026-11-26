//! Tier 3: Scenario 2 - Minimal Controls + Auto-Hide Timer + Fullscreen Queue + Queue Reordering
//!
//! Validates seamless interaction and invariants between:
//! - F10: Minimal Controls (no frosted glass card container, below artwork)
//! - F11: Fullscreen Auto-Hide Timer (3.5s inactivity timeout, mouse wake, hover protection)
//! - F12: Fullscreen Queue Integration (toggle panel, escape dismissal, upcoming tracks)
//! - F13: Fullscreen Queue Reordering (`Queue::move_track` contract, playing index shifts)
//! under active playback, user queue reordering, and timer transitions.

use egui::{pos2, vec2, Context, Key, Pos2, Rect};

use dawnfm::model::Track;
use dawnfm::ui::queue::{Queue, Repeat};

use crate::common::{sample_tracks, HeadlessRunner};

pub const AUTOHIDE_TIMEOUT_SECS: f64 = 3.5;
pub const FADEOUT_DURATION_SECS: f64 = 0.5;

/// Combined Fullscreen Player state controller modeling minimal controls, auto-hide, and queue.
#[derive(Clone, Debug)]
pub struct FullscreenPlayerController {
    // F10 / F11 state
    pub last_mouse_activity: f64,
    pub hovered_controls: bool,
    pub playing: bool,
    pub progress: f32, // 0.0 to 1.0
    pub volume: f32,
    // F12 / F13 state
    pub queue_open: bool,
    pub panel_width: f32,
    pub queue: Queue,
}

impl FullscreenPlayerController {
    pub fn new(tracks: Vec<Track>, start_index: usize) -> Self {
        let mut queue = Queue::new(Repeat::Off);
        if !tracks.is_empty() {
            queue.adopt(tracks, start_index);
        }
        Self {
            last_mouse_activity: 0.0,
            hovered_controls: false,
            playing: true,
            progress: 0.0,
            volume: 0.8,
            queue_open: false,
            panel_width: 360.0,
            queue,
        }
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.queue.current()
    }

    pub fn upcoming_tracks(&self) -> &[Track] {
        if let Some(i) = self.queue.index {
            if i + 1 < self.queue.tracks.len() {
                &self.queue.tracks[i + 1..]
            } else {
                &[]
            }
        } else {
            &self.queue.tracks[..]
        }
    }

    pub fn toggle_queue(&mut self, current_time: f64) {
        self.queue_open = !self.queue_open;
        self.last_mouse_activity = current_time;
    }

    pub fn on_mouse_move(&mut self, current_time: f64, pos: Pos2, controls_rect: Rect, viewport: Rect) {
        self.last_mouse_activity = current_time;
        self.hovered_controls = controls_rect.contains(pos);

        // If mouse is inside open queue panel, it also keeps controls alive
        let queue_rect = self.queue_panel_rect(viewport);
        let in_queue = queue_rect.map(|r| r.contains(pos)).unwrap_or(false);
        if in_queue {
            self.last_mouse_activity = current_time;
        }
    }

    pub fn controls_opacity(&self, current_time: f64) -> f32 {
        if self.hovered_controls {
            return 1.0;
        }
        let elapsed = current_time - self.last_mouse_activity;
        if elapsed <= AUTOHIDE_TIMEOUT_SECS {
            1.0
        } else if elapsed >= AUTOHIDE_TIMEOUT_SECS + FADEOUT_DURATION_SECS {
            0.0
        } else {
            let fade_progress = (elapsed - AUTOHIDE_TIMEOUT_SECS) / FADEOUT_DURATION_SECS;
            (1.0 - fade_progress as f32).clamp(0.0, 1.0)
        }
    }

    pub fn handle_escape(&mut self, ctx: &Context) {
        if self.queue_open && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.queue_open = false;
        }
    }

    pub fn queue_panel_rect(&self, viewport: Rect) -> Option<Rect> {
        if self.queue_open {
            Some(Rect::from_min_size(
                pos2(viewport.max.x - self.panel_width, viewport.min.y),
                vec2(self.panel_width, viewport.height()),
            ))
        } else {
            None
        }
    }

    /// Moves a track from index `from` to index `to`, properly adjusting `playing_index`.
    pub fn move_track(&mut self, from: usize, to: usize, current_time: f64) -> bool {
        self.last_mouse_activity = current_time;
        self.queue.move_track(from, to)
    }
}

#[test]
fn test_fullscreen_queue_interaction_resets_autohide_timer() {
    let mut ctrl = FullscreenPlayerController::new(sample_tracks(10), 0);
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1920.0, 1080.0));
    let controls_rect = Rect::from_min_size(pos2(720.0, 950.0), vec2(480.0, 60.0));

    // Open the queue panel at t = 1.0s
    ctrl.toggle_queue(1.0);
    assert!(ctrl.queue_open);
    assert_eq!(ctrl.controls_opacity(1.0), 1.0);

    // Near timeout boundary at t = 3.0s, user interacts inside queue panel (x = 1800, y = 500)
    ctrl.on_mouse_move(3.0, pos2(1800.0, 500.0), controls_rect, viewport);

    // At t = 5.0s (2.0s after queue activity, but 4.0s after initial t=1.0s)
    // Controls must remain fully visible because queue activity refreshed the timer
    assert_eq!(
        ctrl.controls_opacity(5.0),
        1.0,
        "User activity inside queue panel must refresh auto-hide timer"
    );

    // Inactivity until t = 7.5s (4.5s of no activity since t = 3.0s)
    // Controls must fade to 0.0
    assert_eq!(
        ctrl.controls_opacity(7.5),
        0.0,
        "Controls must be fully hidden after inactivity passes"
    );
}

#[test]
fn test_queue_reorder_preserves_playback_and_scrub_position() {
    let mut ctrl = FullscreenPlayerController::new(sample_tracks(8), 2);
    let initial_track_id = ctrl.current_track().unwrap().id.clone();
    assert_eq!(initial_track_id, "test_track_0002");

    // Playback active at 65% scrub progress
    ctrl.progress = 0.65;
    ctrl.volume = 0.85;

    // User moves track 6 to position 1 (from > playing_index, to <= playing_index)
    let reordered = ctrl.move_track(6, 1, 2.0);
    assert!(reordered);

    // Playing index must have shifted right from 2 to 3
    assert_eq!(ctrl.queue.index, Some(3));
    assert_eq!(
        ctrl.current_track().unwrap().id,
        initial_track_id,
        "Playing track identity must be preserved across shift"
    );

    // Scrub position and volume must remain intact
    assert_eq!(ctrl.progress, 0.65);
    assert_eq!(ctrl.volume, 0.85);
    assert!(ctrl.playing);
}

#[test]
fn test_reorder_moving_active_playing_track_in_fullscreen() {
    let mut ctrl = FullscreenPlayerController::new(sample_tracks(6), 1);
    let playing_id = ctrl.current_track().unwrap().id.clone();
    assert_eq!(playing_id, "test_track_0001");

    ctrl.progress = 0.42;

    // User drags active playing track from index 1 to position 4
    let moved = ctrl.move_track(1, 4, 1.5);
    assert!(moved);

    // Playing index must follow the track to index 4
    assert_eq!(ctrl.queue.index, Some(4));
    assert_eq!(ctrl.current_track().unwrap().id, playing_id);

    // Upcoming tracks must now only contain track 5
    let upcoming = ctrl.upcoming_tracks();
    assert_eq!(upcoming.len(), 1);
    assert_eq!(upcoming[0].id, "test_track_0005");

    // Playback scrub bar remains preserved
    assert_eq!(ctrl.progress, 0.42);
}

#[test]
fn test_escape_key_dismissal_preserves_minimal_controls_and_hover() {
    let mut runner = HeadlessRunner::new();
    let mut ctrl = FullscreenPlayerController::new(sample_tracks(5), 0);
    let controls_rect = Rect::from_min_size(pos2(400.0, 700.0), vec2(400.0, 60.0));
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 780.0));

    // Open queue and hover controls
    ctrl.queue_open = true;
    ctrl.on_mouse_move(0.0, pos2(500.0, 720.0), controls_rect, viewport);
    assert!(ctrl.hovered_controls);

    // Advance virtual time to 10.0s and press Escape
    runner.set_time(10.0);
    runner.press_key(Key::Escape);

    runner.run_frame(|ctx| {
        ctrl.handle_escape(ctx);
    });

    // Queue panel must be dismissed
    assert!(!ctrl.queue_open, "Escape key must dismiss fullscreen queue");

    // Minimal controls must remain fully visible due to hover protection
    assert_eq!(
        ctrl.controls_opacity(10.0),
        1.0,
        "Hovered controls must stay at opacity 1.0 even after escape dismissal"
    );
}

#[test]
fn test_rapid_queue_reordering_under_autohide_fadeout() {
    let mut ctrl = FullscreenPlayerController::new(sample_tracks(10), 0);
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1920.0, 1080.0));
    let controls_rect = Rect::from_min_size(pos2(720.0, 950.0), vec2(480.0, 60.0));

    // Inactivity leads to mid-fadeout at t = 3.75s
    ctrl.last_mouse_activity = 0.0;
    let fade_opacity = ctrl.controls_opacity(3.75);
    assert!(
        fade_opacity > 0.0 && fade_opacity < 1.0,
        "Controls must be actively fading at t=3.75s"
    );

    // User performs quick reordering at t = 3.75s
    ctrl.on_mouse_move(3.75, pos2(1800.0, 400.0), controls_rect, viewport);
    let moved = ctrl.move_track(4, 1, 3.75);
    assert!(moved);

    // Controls must immediately snap back to full opacity 1.0
    assert_eq!(
        ctrl.controls_opacity(3.75),
        1.0,
        "Reorder interaction must wake controls immediately to opacity 1.0"
    );
}
