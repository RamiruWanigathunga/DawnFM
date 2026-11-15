//! Tier 2: F12 - Fullscreen Queue Integration Boundaries
//!
//! Validates empty queue state, 10,000-track virtualization slice calculation,
//! 100-cycle rapid open/close toggling, viewport resize while open,
//! and outside click detection per ORIGINAL_REQUEST §R1.

use egui::{pos2, vec2, Pos2, Rect};

use dawnfm::model::Track;

use crate::common::sample_tracks;

#[derive(Clone, Debug)]
pub struct FullscreenQueueModel {
    pub open: bool,
    pub tracks: Vec<Track>,
    pub index: Option<usize>,
    pub panel_width: f32,
}

impl FullscreenQueueModel {
    pub fn new() -> Self {
        Self {
            open: false,
            tracks: Vec::new(),
            index: None,
            panel_width: 360.0,
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn panel_rect(&self, viewport: Rect) -> Option<Rect> {
        if !self.open {
            return None;
        }
        let w = self.panel_width.min(viewport.width()).max(0.0);
        let h = viewport.height().max(0.0);
        let x = viewport.max.x - w;
        let y = viewport.min.y;
        Some(Rect::from_min_size(pos2(x, y), vec2(w, h)))
    }

    pub fn visible_track_range(&self, row_height: f32, scroll_y: f32, viewport_height: f32) -> (usize, usize) {
        if self.tracks.is_empty() || row_height <= 0.0 {
            return (0, 0);
        }
        let start = ((scroll_y / row_height).floor() as usize).min(self.tracks.len());
        let count = ((viewport_height / row_height).ceil() as usize) + 2; // Buffer extra rows
        let end = (start + count).min(self.tracks.len());
        (start, end)
    }

    pub fn handle_click(&mut self, click_pos: Pos2, viewport: Rect) -> bool {
        if let Some(panel) = self.panel_rect(viewport) {
            if !panel.contains(click_pos) {
                // Click outside dismisses queue panel
                self.open = false;
                return true;
            }
        }
        false
    }
}

impl Default for FullscreenQueueModel {
    fn default() -> Self {
        Self::new()
    }
}

#[test]
fn test_fs_queue_empty_queue_state() {
    let mut model = FullscreenQueueModel::new();
    model.open = true;

    assert!(model.tracks.is_empty());
    assert_eq!(model.index, None);

    let (start, end) = model.visible_track_range(50.0, 0.0, 780.0);
    assert_eq!(start, 0);
    assert_eq!(end, 0);
}

#[test]
fn test_fs_queue_10000_tracks_virtualization() {
    let mut model = FullscreenQueueModel::new();
    model.open = true;
    model.tracks = sample_tracks(10_000);
    assert_eq!(model.tracks.len(), 10_000);

    // Visible window at scroll_y = 5000.0 (row height = 50px)
    let (start, end) = model.visible_track_range(50.0, 5000.0, 800.0);
    assert_eq!(start, 100);
    // 800 / 50 = 16 rows + 2 buffer rows = 18 rows
    assert_eq!(end, 118);

    // Verify subslice is O(1) and does not clone 10,000 items
    let visible_slice = &model.tracks[start..end];
    assert_eq!(visible_slice.len(), 18);
    assert_eq!(visible_slice[0].id, "test_track_0100");
}

#[test]
fn test_fs_queue_rapid_open_close_toggling() {
    let mut model = FullscreenQueueModel::new();
    assert!(!model.open);

    for i in 0..100 {
        model.toggle();
        assert_eq!(model.open, i % 2 == 0);
    }
    assert!(!model.open);
}

#[test]
fn test_fs_queue_window_resize_while_open() {
    let mut model = FullscreenQueueModel::new();
    model.open = true;
    model.panel_width = 360.0;

    // Standard desktop (1920x1080)
    let desktop = Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0));
    let panel_desktop = model.panel_rect(desktop).unwrap();
    assert_eq!(panel_desktop.width(), 360.0);

    // Resized to narrow window (300x400)
    let narrow = Rect::from_min_size(Pos2::ZERO, vec2(300.0, 400.0));
    let panel_narrow = model.panel_rect(narrow).unwrap();
    assert_eq!(panel_narrow.width(), 300.0, "Panel width must clamp to viewport width");
    assert_eq!(panel_narrow.max.x, 300.0);
}

#[test]
fn test_fs_queue_click_outside_detection() {
    let mut model = FullscreenQueueModel::new();
    model.open = true;
    model.panel_width = 360.0;

    let viewport = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
    // Panel rect spans x from 840.0 to 1200.0

    // Click inside panel at (900, 200) -> does not dismiss
    let dismissed1 = model.handle_click(pos2(900.0, 200.0), viewport);
    assert!(!dismissed1);
    assert!(model.open);

    // Click outside panel on the left at (400, 200) -> dismisses
    let dismissed2 = model.handle_click(pos2(400.0, 200.0), viewport);
    assert!(dismissed2);
    assert!(!model.open);
}
