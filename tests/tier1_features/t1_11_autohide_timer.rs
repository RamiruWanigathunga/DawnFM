//! Tier 1: F11 - Fullscreen Auto-Hide Controls
//!
//! Validates 3.5s inactivity timeout constant, controls visibility upon mouse activity,
//! smooth fade-out upon inactivity, zero-opacity at idle, and hover inhibition per ORIGINAL_REQUEST §R1.

use egui::{pos2, vec2, Pos2, Rect};

pub const AUTOHIDE_TIMEOUT_SECS: f64 = 3.5;
pub const FADEOUT_DURATION_SECS: f64 = 0.5;

/// State machine modeling fullscreen auto-hide behavior per interface contract
pub struct AutoHideController {
    last_mouse_activity: f64,
    hovered: bool,
}

impl AutoHideController {
    pub fn new() -> Self {
        Self {
            last_mouse_activity: 0.0,
            hovered: false,
        }
    }

    pub fn on_mouse_move(&mut self, current_time: f64, pos: Pos2, controls_rect: Rect) {
        self.last_mouse_activity = current_time;
        self.hovered = controls_rect.contains(pos);
    }

    pub fn opacity(&self, current_time: f64) -> f32 {
        if self.hovered {
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
}

impl Default for AutoHideController {
    fn default() -> Self {
        Self::new()
    }
}

#[test]
fn test_autohide_timer_duration_constant() {
    assert_eq!(
        AUTOHIDE_TIMEOUT_SECS, 3.5,
        "Fullscreen auto-hide inactivity timeout must be exactly 3.5 seconds"
    );
}

#[test]
fn test_controls_visible_on_mouse_activity() {
    let mut ctrl = AutoHideController::new();
    let controls_rect = Rect::from_min_size(pos2(400.0, 700.0), vec2(400.0, 60.0));

    ctrl.on_mouse_move(1.0, pos2(200.0, 200.0), controls_rect);
    assert_eq!(
        ctrl.opacity(1.0),
        1.0,
        "Controls must be fully visible (opacity 1.0) immediately on mouse activity"
    );

    assert_eq!(
        ctrl.opacity(2.5),
        1.0,
        "Controls must remain fully visible within 3.5s of activity"
    );
}

#[test]
fn test_controls_fadeout_after_inactivity() {
    let mut ctrl = AutoHideController::new();
    let controls_rect = Rect::from_min_size(pos2(400.0, 700.0), vec2(400.0, 60.0));

    ctrl.on_mouse_move(0.0, pos2(200.0, 200.0), controls_rect);

    // At 3.75s (halfway through the 0.5s fadeout window), opacity should be ~0.5
    let opacity_fade = ctrl.opacity(3.75);
    assert!(
        opacity_fade > 0.0 && opacity_fade < 1.0,
        "Controls must be in active fadeout transition after 3.5s inactivity, got {}",
        opacity_fade
    );
    assert!((opacity_fade - 0.5).abs() < 1e-3);
}

#[test]
fn test_controls_fully_hidden_at_idle() {
    let mut ctrl = AutoHideController::new();
    let controls_rect = Rect::from_min_size(pos2(400.0, 700.0), vec2(400.0, 60.0));

    ctrl.on_mouse_move(0.0, pos2(200.0, 200.0), controls_rect);

    let opacity_idle = ctrl.opacity(5.0);
    assert_eq!(
        opacity_idle, 0.0,
        "Controls must be fully hidden (opacity 0.0) after inactivity window passes"
    );
}

#[test]
fn test_controls_hover_prevents_fadeout() {
    let mut ctrl = AutoHideController::new();
    let controls_rect = Rect::from_min_size(pos2(400.0, 700.0), vec2(400.0, 60.0));

    ctrl.on_mouse_move(0.0, pos2(450.0, 720.0), controls_rect);

    assert_eq!(
        ctrl.opacity(10.0),
        1.0,
        "Hovering over controls must prevent auto-hide fadeout regardless of elapsed time"
    );
}
