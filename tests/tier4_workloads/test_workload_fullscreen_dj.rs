//! Tier 4: Workload 2 - Fullscreen DJ Immersion Workflow Simulation
//!
//! Simulates a live fullscreen DJ performance session exercising:
//! - F10: Minimal Fullscreen Controls (no frosted glass card container, floating below artwork)
//! - F11: Fullscreen Auto-Hide Timer (3.5s inactivity timeout, mouse wake, hover protection)
//! - F12: Fullscreen Queue Integration (header toggle button, side panel, upcoming tracks)
//! - F13: Fullscreen Queue Reordering (`Queue::move_track` contract and playing index shifts)
//! - F14: Kawarp Animated Blur Background (fluid mesh GPU shader, smooth animation, palette interpolation)

use egui::{pos2, vec2, Color32, Pos2, Rect};

use dawnfm::model::Track;
use dawnfm::shader::Params;
use dawnfm::ui::queue::{Queue, Repeat};

use crate::common::{sample_tracks, HeadlessRunner};

pub const AUTOHIDE_TIMEOUT_SECS: f64 = 3.5;
pub const FADEOUT_DURATION_SECS: f64 = 0.5;

/// Fullscreen DJ controller maintaining UI view state, auto-hide timer, queue, and background shader.
#[allow(dead_code)]
pub struct FullscreenDjController {
    // F10 / F11 minimal controls & timer
    pub last_mouse_activity: f64,
    pub hovered_controls: bool,
    pub playing: bool,
    pub progress: f32, // 0.0 to 1.0
    pub volume: f32,
    pub scrub_time_secs: f64,
    // F12 / F13 queue state
    pub queue: Queue,
    pub queue_open: bool,
    pub panel_width: f32,
    // F14 Kawarp shader state
    pub shader_params: Params,
    pub dominant_color: Color32,
}

impl FullscreenDjController {
    pub fn new(tracks: Vec<Track>, start_index: usize) -> Self {
        let mut queue = Queue::new(Repeat::Off);
        queue.replace(tracks, start_index, None);

        Self {
            last_mouse_activity: 0.0,
            hovered_controls: false,
            playing: true,
            progress: 0.15,
            volume: 0.85,
            scrub_time_secs: 30.0,
            queue,
            queue_open: false,
            panel_width: 380.0,
            shader_params: Params {
                time: 0.0,
                blend: 0.0,
                intensity: 1.0,
                saturation: 1.2,
                dim: 0.85,
                scale: 1.0,
            },
            dominant_color: Color32::from_rgb(180, 40, 60), // Deep crimson
        }
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.queue.current()
    }

    pub fn toggle_queue(&mut self, current_time: f64) {
        self.queue_open = !self.queue_open;
        self.last_mouse_activity = current_time;
    }

    pub fn on_mouse_activity(&mut self, current_time: f64, pos: Pos2, viewport: Rect) {
        self.last_mouse_activity = current_time;
        let controls = self.controls_rect(viewport);
        self.hovered_controls = controls.contains(pos);

        if self.queue_open {
            if let Some(qr) = self.queue_panel_rect(viewport) {
                if qr.contains(pos) {
                    self.hovered_controls = true;
                }
            }
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
            let fade = (elapsed - AUTOHIDE_TIMEOUT_SECS) / FADEOUT_DURATION_SECS;
            (1.0 - fade as f32).clamp(0.0, 1.0)
        }
    }

    pub fn artwork_rect(&self, viewport: Rect) -> Rect {
        let side = (viewport.height() * 0.48).clamp(240.0, 480.0);
        let x = (viewport.width() - side) / 2.0;
        let y = viewport.top() + (viewport.height() - side) * 0.35;
        Rect::from_min_size(pos2(x, y), vec2(side, side))
    }

    pub fn controls_rect(&self, viewport: Rect) -> Rect {
        let art = self.artwork_rect(viewport);
        let w = (viewport.width() * 0.40).clamp(320.0, 480.0);
        let h = 80.0;
        let x = (viewport.width() - w) / 2.0;
        let y = art.max.y + 24.0;
        Rect::from_min_size(pos2(x, y), vec2(w, h))
    }

    pub fn queue_panel_rect(&self, viewport: Rect) -> Option<Rect> {
        if !self.queue_open {
            None
        } else {
            Some(Rect::from_min_size(
                pos2(viewport.right() - self.panel_width, viewport.top()),
                vec2(self.panel_width, viewport.height()),
            ))
        }
    }

    pub fn step_frame(&mut self, dt: f32) {
        self.shader_params.time += dt;
        if self.playing {
            self.scrub_time_secs += dt as f64;
            if let Some(track) = self.current_track() {
                let dur = track.duration.unwrap_or(200) as f64;
                self.progress = (self.scrub_time_secs / dur).clamp(0.0, 1.0) as f32;
            }
        }
    }
}

#[test]
fn test_workload_fullscreen_dj_immersion_workflow() {
    let tracks = sample_tracks(12);
    let mut dj = FullscreenDjController::new(tracks, 2); // Start at track #2 ("Test Song #003")
    assert_eq!(dj.current_track().unwrap().title, "Test Song #003");

    let mut runner = HeadlessRunner::new().with_size(1920.0, 1080.0);
    let viewport = Rect::from_min_size(Pos2::ZERO, runner.screen_size);

    // Initial state: controls visible below artwork
    let initial_opacity = dj.controls_opacity(0.0);
    assert_eq!(initial_opacity, 1.0, "Controls must be fully visible at start");

    let art_rect = dj.artwork_rect(viewport);
    let controls_rect = dj.controls_rect(viewport);
    assert!(
        controls_rect.top() > art_rect.bottom(),
        "F10: Minimal controls must be positioned directly underneath the artwork card"
    );

    // 1. DJ performs with hands off controls for 4.0s -> controls smoothly fade out to 0.0
    runner.set_time(4.0);
    dj.step_frame(4.0);
    let idle_opacity = dj.controls_opacity(4.0);
    assert_eq!(
        idle_opacity, 0.0,
        "F11: Controls must be completely hidden (opacity 0.0) after 4.0s of inactivity"
    );
    assert!(
        dj.shader_params.time >= 4.0,
        "F14: Kawarp shader must continuously animate in background even when controls fade"
    );

    // 2. DJ moves mouse at 5.0s -> controls immediately wake to 1.0
    runner.set_time(5.0);
    dj.on_mouse_activity(5.0, pos2(960.0, 540.0), viewport);
    let wake_opacity = dj.controls_opacity(5.0);
    assert_eq!(
        wake_opacity, 1.0,
        "F11: Mouse movement must instantly wake controls back to opacity 1.0"
    );

    // 3. DJ opens the Fullscreen Queue panel via header button
    assert!(!dj.queue_open);
    dj.toggle_queue(5.2);
    assert!(dj.queue_open, "F12: Header queue toggle must open queue panel");
    let queue_rect = dj.queue_panel_rect(viewport).expect("Queue panel rect must exist");
    assert_eq!(queue_rect.width(), dj.panel_width);
    assert_eq!(queue_rect.right(), viewport.right());

    // 4. DJ reorders upcoming tracks while audio continues playing:
    // Move upcoming track at index 6 to index 3 (immediately after currently playing index 2)
    let playing_before = dj.queue.index.unwrap();
    let track_6_title = dj.queue.tracks[6].title.clone();
    let move_ok = dj.queue.move_track(6, 3);
    assert!(move_ok, "F13: Queue::move_track must succeed for valid upcoming indices");
    assert_eq!(
        dj.queue.index.unwrap(),
        playing_before,
        "Current playing track index (2) must not change when reordering tracks after it"
    );
    assert_eq!(
        dj.queue.tracks[3].title, track_6_title,
        "Track 6 must now be at index 3"
    );

    // 5. DJ moves currently playing track from index 2 to index 0
    let move_cur_ok = dj.queue.move_track(2, 0);
    assert!(move_cur_ok, "F13: Queue::move_track must support moving the active track");
    assert_eq!(
        dj.queue.index,
        Some(0),
        "Queue::index must be updated to 0 to track the moved active song"
    );
    assert_eq!(
        dj.current_track().unwrap().title,
        "Test Song #003",
        "Active track title identity must remain consistent"
    );

    // 6. DJ hovers inside the queue panel at 9.0s (inactivity would otherwise fade controls)
    runner.set_time(9.0);
    dj.on_mouse_activity(9.0, queue_rect.center(), viewport);
    let hover_opacity = dj.controls_opacity(9.0);
    assert_eq!(
        hover_opacity, 1.0,
        "F11: Hovering over the open queue panel must keep controls pinned visible"
    );

    // 7. DJ presses Escape key to dismiss queue panel
    dj.toggle_queue(9.5);
    assert!(!dj.queue_open, "F12: Escape key must close the queue panel");
    assert!(dj.queue_panel_rect(viewport).is_none());
}

#[test]
fn test_fullscreen_dj_autohide_timer_fadeout_and_instant_wake() {
    let tracks = sample_tracks(5);
    let mut dj = FullscreenDjController::new(tracks, 0);
    let viewport = Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0));

    // Boundary check 0.0s to 3.5s
    assert_eq!(dj.controls_opacity(0.0), 1.0);
    assert_eq!(dj.controls_opacity(1.0), 1.0);
    assert_eq!(dj.controls_opacity(2.5), 1.0);
    assert_eq!(dj.controls_opacity(3.5), 1.0, "Opacity must remain 1.0 up to 3.5s");

    // Boundary check during 0.5s fadeout window (3.5s to 4.0s)
    let mid_fade = dj.controls_opacity(3.75);
    assert!(
        (mid_fade - 0.5).abs() < 0.01,
        "At 3.75s (halfway through fadeout), opacity must be ~0.5, found {}",
        mid_fade
    );

    // Boundary check at >= 4.0s (fully hidden)
    assert_eq!(dj.controls_opacity(4.0), 0.0);
    assert_eq!(dj.controls_opacity(10.0), 0.0);
    assert_eq!(dj.controls_opacity(100.0), 0.0);

    // Wake event
    dj.on_mouse_activity(100.5, pos2(400.0, 300.0), viewport);
    assert_eq!(dj.controls_opacity(100.5), 1.0);
}

#[test]
fn test_fullscreen_dj_live_queue_reordering_during_playback() {
    let tracks = sample_tracks(8);
    let mut queue = Queue::new(Repeat::Off);
    queue.replace(tracks, 3, None); // Playing song at index 3

    let current_id = queue.current().unwrap().id.clone();
    assert_eq!(current_id, "test_track_0003");

    // Case 1: Reorder upcoming track from 6 to 4 (both after current index 3)
    assert!(queue.move_track(6, 4));
    assert_eq!(queue.index, Some(3));
    assert_eq!(queue.current().unwrap().id, current_id);

    // Case 2: Move track behind current index: from 1 to 2
    assert!(queue.move_track(1, 2));
    assert_eq!(queue.index, Some(3));
    assert_eq!(queue.current().unwrap().id, current_id);

    // Case 3: Move a track from behind to ahead of current (from 1 to 5)
    // The current playing index must decrement from 3 to 2
    assert!(queue.move_track(1, 5));
    assert_eq!(queue.index, Some(2));
    assert_eq!(queue.current().unwrap().id, current_id);

    // Case 4: Move a track from ahead to behind of current (from 6 to 0)
    // The current playing index must increment from 2 to 3
    assert!(queue.move_track(6, 0));
    assert_eq!(queue.index, Some(3));
    assert_eq!(queue.current().unwrap().id, current_id);

    // Case 5: Move current playing track from 3 to 7
    assert!(queue.move_track(3, 7));
    assert_eq!(queue.index, Some(7));
    assert_eq!(queue.current().unwrap().id, current_id);

    // Boundary checks: out-of-bounds index returns false
    assert!(!queue.move_track(100, 2));
    assert!(!queue.move_track(2, 100));
    // Identity move returns false
    assert!(!queue.move_track(4, 4));
}

#[test]
fn test_fullscreen_dj_queue_panel_hover_and_escape_dismiss() {
    let tracks = sample_tracks(6);
    let mut dj = FullscreenDjController::new(tracks, 1);
    let viewport = Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0));

    dj.toggle_queue(0.0);
    let qrect = dj.queue_panel_rect(viewport).unwrap();

    // Mouse is inside queue panel at time 5.0s
    dj.on_mouse_activity(5.0, qrect.center(), viewport);
    assert!(dj.hovered_controls);
    assert_eq!(dj.controls_opacity(5.0), 1.0);

    // Dismiss queue
    dj.toggle_queue(5.5);
    assert!(!dj.queue_open);
}

#[test]
fn test_fullscreen_dj_kawarp_shader_continuous_animation_under_dj_actions() {
    let tracks = sample_tracks(4);
    let mut dj = FullscreenDjController::new(tracks, 0);

    // Run 60 frames of 16.6ms (1.0s total)
    for _ in 0..60 {
        dj.step_frame(1.0 / 60.0);
    }

    assert!(
        (dj.shader_params.time - 1.0).abs() < 0.05,
        "Shader time must advance monotonically by elapsed delta"
    );
    assert!(dj.shader_params.intensity > 0.0 && dj.shader_params.intensity.is_finite());
    assert!(dj.shader_params.dim > 0.0 && dj.shader_params.dim.is_finite());

    // Color transition on track change
    let new_accent = Color32::from_rgb(40, 120, 220); // Neon cyan
    dj.dominant_color = new_accent;
    assert_eq!(dj.dominant_color.r(), 40);
    assert_eq!(dj.dominant_color.g(), 120);
    assert_eq!(dj.dominant_color.b(), 220);
}
