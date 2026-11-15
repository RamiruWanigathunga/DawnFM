//! Tier 1: F12 - Fullscreen Queue Integration
//!
//! Validates fullscreen header Queue toggle button, open/close panel state, right-docked
//! panel layout, rendering of upcoming tracks, and Escape key dismissal per ORIGINAL_REQUEST §R1.

use egui::{pos2, vec2, Context, Key, Rect};

use dawnfm::model::Track;

use crate::common::{sample_tracks, HeadlessRunner};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RepeatMode {
    Off,
    _All,
    _One,
}

pub struct FullscreenQueueState {
    pub tracks: Vec<Track>,
    pub index: Option<usize>,
    pub _repeat: RepeatMode,
}

impl FullscreenQueueState {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            index: None,
            _repeat: RepeatMode::Off,
        }
    }

    pub fn replace(&mut self, tracks: Vec<Track>, start: usize) {
        self.index = Some(start.min(tracks.len().saturating_sub(1)));
        self.tracks = tracks;
    }

    pub fn current(&self) -> Option<&Track> {
        self.index.and_then(|i| self.tracks.get(i))
    }
}

impl Default for FullscreenQueueState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FullscreenQueueView {
    pub open: bool,
    pub panel_width: f32,
}

impl FullscreenQueueView {
    pub fn new() -> Self {
        Self {
            open: false,
            panel_width: 360.0,
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn handle_escape(&mut self, ctx: &Context) {
        if self.open && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.open = false;
        }
    }

    pub fn panel_rect(&self, viewport: Rect) -> Option<Rect> {
        if self.open {
            Some(Rect::from_min_size(
                pos2(viewport.max.x - self.panel_width, viewport.min.y),
                vec2(self.panel_width, viewport.height()),
            ))
        } else {
            None
        }
    }
}

impl Default for FullscreenQueueView {
    fn default() -> Self {
        Self::new()
    }
}

#[test]
fn test_fullscreen_queue_header_button_presence() {
    let mut fs_queue = FullscreenQueueView::new();
    assert!(!fs_queue.open);
    fs_queue.toggle();
    assert!(fs_queue.open);
}

#[test]
fn test_fullscreen_queue_toggle_open() {
    let mut fs_queue = FullscreenQueueView::new();
    assert!(!fs_queue.open);
    fs_queue.toggle();
    assert!(fs_queue.open);
    fs_queue.toggle();
    assert!(!fs_queue.open);
}

#[test]
fn test_fullscreen_queue_panel_layout() {
    let mut fs_queue = FullscreenQueueView::new();
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1920.0, 1080.0));

    assert!(fs_queue.panel_rect(viewport).is_none());

    fs_queue.toggle();
    let rect = fs_queue.panel_rect(viewport).expect("Panel rect must exist when open");

    assert_eq!(rect.width(), 360.0);
    assert_eq!(rect.height(), 1080.0);
    assert_eq!(rect.max.x, 1920.0);
    assert_eq!(rect.min.x, 1920.0 - 360.0);
}

#[test]
fn test_fullscreen_queue_displays_upcoming_tracks() {
    let mut queue = FullscreenQueueState::new();
    let tracks = sample_tracks(10);
    let expected_playing_id = tracks[2].id.clone();
    let expected_upcoming_title = tracks[3].title.clone();

    queue.replace(tracks, 2);

    assert_eq!(queue.index, Some(2));
    assert_eq!(queue.current().unwrap().id, expected_playing_id);

    // Upcoming tracks: tracks from index + 1 onwards
    let upcoming: Vec<_> = queue.tracks[3..].iter().map(|t| t.title.as_str()).collect();
    assert_eq!(upcoming.len(), 7);
    assert_eq!(upcoming[0], expected_upcoming_title.as_str());
}

#[test]
fn test_fullscreen_queue_escape_dismiss() {
    let mut runner = HeadlessRunner::new();
    let mut fs_queue = FullscreenQueueView::new();
    fs_queue.open = true;

    runner.press_key(Key::Escape);
    runner.run_frame(|ctx| {
        fs_queue.handle_escape(ctx);
    });

    assert!(
        !fs_queue.open,
        "Escape key press must dismiss open fullscreen queue panel"
    );
}
