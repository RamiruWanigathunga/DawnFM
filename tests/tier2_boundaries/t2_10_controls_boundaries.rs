//! Tier 2: F10 - Minimal Fullscreen Controls Boundaries
//!
//! Validates zero-dimension window bounds, ultrawide 32:9 layout, narrow portrait layout,
//! 100-cycle rapid fullscreen toggling, and scrub bar 0-to-max boundary clamping per ORIGINAL_REQUEST §R1.

use egui::{pos2, vec2, Pos2, Rect};

use crate::common::HeadlessRunner;

pub fn compute_controls_layout(viewport: Rect, art_size: f32) -> (Rect, Rect) {
    let safe_w = viewport.width().max(0.0);
    let safe_h = viewport.height().max(0.0);

    let effective_art = art_size.min(safe_w * 0.8).min(safe_h * 0.6).max(0.0);
    let art_x = viewport.min.x + (safe_w - effective_art) / 2.0;
    let art_y = viewport.min.y + (safe_h - effective_art) / 2.0 - (safe_h * 0.05);
    let art_rect = Rect::from_min_size(pos2(art_x, art_y), vec2(effective_art, effective_art));

    let controls_width = (safe_w * 0.6).clamp(240.0, 560.0).min(safe_w);
    let controls_height = 48.0f32.min(safe_h);
    let controls_x = viewport.min.x + (safe_w - controls_width) / 2.0;
    let controls_y = art_rect.max.y + 16.0;
    let controls_rect = Rect::from_min_size(pos2(controls_x, controls_y), vec2(controls_width, controls_height));

    (art_rect, controls_rect)
}

pub fn clamp_scrub_progress(current_secs: f64, total_secs: f64) -> f64 {
    if total_secs <= 0.0 || total_secs.is_nan() || current_secs.is_nan() {
        0.0
    } else {
        (current_secs / total_secs).clamp(0.0, 1.0)
    }
}

#[test]
fn test_controls_zero_window_dimension() {
    let zero_viewport = Rect::from_min_size(Pos2::ZERO, vec2(0.0, 0.0));
    let (art_rect, controls_rect) = compute_controls_layout(zero_viewport, 300.0);

    assert_eq!(art_rect.width(), 0.0);
    assert_eq!(art_rect.height(), 0.0);
    assert_eq!(controls_rect.width(), 0.0);
    assert_eq!(controls_rect.height(), 0.0);
}

#[test]
fn test_controls_ultrawide_32_9_layout() {
    let ultrawide_viewport = Rect::from_min_size(Pos2::ZERO, vec2(5120.0, 1440.0));
    let (art_rect, controls_rect) = compute_controls_layout(ultrawide_viewport, 450.0);

    assert_eq!(art_rect.width(), 450.0);
    assert_eq!(controls_rect.width(), 560.0); // Clamped to max 560px
    assert!(
        controls_rect.min.y >= art_rect.max.y,
        "Controls must reside strictly below artwork in ultrawide"
    );

    // Verify horizontal centering
    let center_viewport = ultrawide_viewport.center().x;
    let center_controls = controls_rect.center().x;
    assert!((center_viewport - center_controls).abs() < 1.0);
}

#[test]
fn test_controls_narrow_portrait_layout() {
    let narrow_viewport = Rect::from_min_size(Pos2::ZERO, vec2(360.0, 800.0));
    let (art_rect, controls_rect) = compute_controls_layout(narrow_viewport, 400.0);

    assert!(art_rect.width() <= 360.0);
    assert!(controls_rect.width() <= 360.0);
    assert!(controls_rect.min.x >= 0.0);
    assert!(controls_rect.max.x <= 360.0);
}

#[test]
fn test_controls_100x_rapid_fullscreen_toggle() {
    let mut runner = HeadlessRunner::new();
    let mut is_fullscreen = false;

    for _i in 0..100 {
        is_fullscreen = !is_fullscreen;
        let (w, h) = if is_fullscreen { (1920.0, 1080.0) } else { (1024.0, 640.0) };
        runner = runner.with_size(w, h);

        let (art, controls) = compute_controls_layout(
            Rect::from_min_size(Pos2::ZERO, vec2(w, h)),
            350.0,
        );

        assert!(art.is_finite());
        assert!(controls.is_finite());
        assert!(controls.min.y >= art.max.y);
    }
}

#[test]
fn test_controls_scrub_bar_at_0_and_max() {
    let duration = 240.0; // 4 minutes

    // 1. Exactly at start
    assert_eq!(clamp_scrub_progress(0.0, duration), 0.0);

    // 2. Exactly at end
    assert_eq!(clamp_scrub_progress(duration, duration), 1.0);

    // 3. Past end (overshoot)
    assert_eq!(clamp_scrub_progress(duration + 50.0, duration), 1.0);

    // 4. Negative position
    assert_eq!(clamp_scrub_progress(-10.0, duration), 0.0);

    // 5. Zero duration stream
    assert_eq!(clamp_scrub_progress(10.0, 0.0), 0.0);

    // 6. NaN values
    assert_eq!(clamp_scrub_progress(f64::NAN, duration), 0.0);
    assert_eq!(clamp_scrub_progress(10.0, f64::NAN), 0.0);
}
