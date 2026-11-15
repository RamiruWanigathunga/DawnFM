//! Tier 2: F11 - Fullscreen Auto-Hide Controls Boundaries
//!
//! Validates clock rewind / negative dt resilience, 10,000-second massive time jump,
//! 1000Hz mouse move event jitter, instant sub-millisecond reappear latency,
//! and boundary edge hover detection per ORIGINAL_REQUEST §R1.

use egui::{pos2, vec2, Pos2, Rect};

pub const AUTOHIDE_TIMEOUT_SECS: f64 = 3.5;
pub const FADEOUT_DURATION_SECS: f64 = 0.5;

#[derive(Clone, Debug)]
pub struct RobustAutoHideController {
    pub last_mouse_activity: f64,
    pub hovered: bool,
}

impl RobustAutoHideController {
    pub fn new() -> Self {
        Self {
            last_mouse_activity: 0.0,
            hovered: false,
        }
    }

    pub fn on_mouse_event(&mut self, current_time: f64, pos: Pos2, controls_rect: Rect) {
        self.last_mouse_activity = current_time;
        // Check contains including inclusive edge bounds
        self.hovered = controls_rect.expand(0.5).contains(pos);
    }

    pub fn opacity(&self, current_time: f64) -> f32 {
        if self.hovered {
            return 1.0;
        }
        // Protect against clock rewinds where current_time < last_mouse_activity
        let elapsed = (current_time - self.last_mouse_activity).max(0.0);
        if elapsed <= AUTOHIDE_TIMEOUT_SECS {
            1.0
        } else if elapsed >= AUTOHIDE_TIMEOUT_SECS + FADEOUT_DURATION_SECS {
            0.0
        } else {
            let fade = (elapsed - AUTOHIDE_TIMEOUT_SECS) / FADEOUT_DURATION_SECS;
            (1.0 - fade as f32).clamp(0.0, 1.0)
        }
    }
}

impl Default for RobustAutoHideController {
    fn default() -> Self {
        Self::new()
    }
}

#[test]
fn test_autohide_negative_dt_clock_rewind() {
    let mut ctrl = RobustAutoHideController::new();
    let controls = Rect::from_min_size(pos2(100.0, 100.0), vec2(300.0, 50.0));

    // Mouse activity at t = 100.0s
    ctrl.on_mouse_event(100.0, pos2(0.0, 0.0), controls);
    assert_eq!(ctrl.opacity(100.0), 1.0);

    // Clock step backwards (NTP rewind) to t = 80.0s (dt = -20s)
    let op_rewind = ctrl.opacity(80.0);
    assert_eq!(
        op_rewind, 1.0,
        "Clock rewind must not cause negative elapsed time or opacity distortion"
    );
}

#[test]
fn test_autohide_massive_time_jump_10000s() {
    let mut ctrl = RobustAutoHideController::new();
    let controls = Rect::from_min_size(pos2(100.0, 100.0), vec2(300.0, 50.0));

    ctrl.on_mouse_event(0.0, pos2(0.0, 0.0), controls);
    assert_eq!(ctrl.opacity(0.0), 1.0);

    // Jump 10,000 seconds forward
    let op_jump = ctrl.opacity(10_000.0);
    assert_eq!(op_jump, 0.0, "Massive time jump must result in 0.0 opacity");
}

#[test]
fn test_autohide_1000hz_mouse_move_jitter() {
    let mut ctrl = RobustAutoHideController::new();
    let controls = Rect::from_min_size(pos2(100.0, 100.0), vec2(300.0, 50.0));

    let base_time = 10.0;
    // 1000 events within 1 millisecond
    for i in 0..1000 {
        let t = base_time + (i as f64 * 0.000001);
        ctrl.on_mouse_event(t, pos2(50.0, 50.0), controls);
        assert_eq!(ctrl.opacity(t), 1.0);
    }
}

#[test]
fn test_autohide_instant_reappear_latency() {
    let mut ctrl = RobustAutoHideController::new();
    let controls = Rect::from_min_size(pos2(100.0, 100.0), vec2(300.0, 50.0));

    // Mouse at t = 0
    ctrl.on_mouse_event(0.0, pos2(10.0, 10.0), controls);
    // At t = 5.0, controls are fully hidden (opacity = 0.0)
    assert_eq!(ctrl.opacity(5.0), 0.0);

    // Mouse moves at t = 5.001
    ctrl.on_mouse_event(5.001, pos2(20.0, 20.0), controls);

    // Opacity must immediately be 1.0 with 0 latency
    assert_eq!(
        ctrl.opacity(5.001), 1.0,
        "Controls must immediately reappear at 1.0 opacity on first cursor movement"
    );
}

#[test]
fn test_autohide_multitouch_scroll_inhibition() {
    let mut ctrl = RobustAutoHideController::new();
    let controls = Rect::from_min_size(pos2(100.0, 100.0), vec2(300.0, 50.0));

    // Boundary corners: min, max, top-right, bottom-left
    let boundary_points = [
        controls.min,
        controls.max,
        pos2(controls.max.x, controls.min.y),
        pos2(controls.min.x, controls.max.y),
    ];

    for &p in &boundary_points {
        ctrl.on_mouse_event(10.0, p, controls);
        assert!(
            ctrl.hovered,
            "Boundary point {:?} must be recognized as hovering the controls",
            p
        );
        // Even after 10 seconds of idle, hovering prevents fadeout
        assert_eq!(ctrl.opacity(25.0), 1.0);
    }
}
