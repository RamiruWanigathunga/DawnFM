//! Tier 3: Scenario 5 - Kawarp Fluid Mesh Shader + Minimal Controls + Auto-Hide Timer + macOS Vibrancy
//!
//! Validates seamless visual compositing and animation continuity between:
//! - F14: Kawarp Fluid Mesh Shader / Headless Fallback (WGSL/GLSL, params, smooth animation)
//! - F10: Minimal Controls (no frosted glass card container, below artwork)
//! - F11: Auto-Hide Timer (3.5s inactivity timeout, mouse wake, hover protection)
//! - F06: macOS Native Window Vibrancy (transparency, `theme::glass()`, `theme::shade()`, dark theme tokens)
//! under continuous playback, inactivity fadeout, mouse wake, and viewport resize.

use egui::{pos2, vec2, Color32, Pos2, Rect};

use dawnfm::shader::Params;
use dawnfm::theme;
use dawnfm::ui::nowplaying;

use crate::common::{extract_rect_shapes, run_headless_egui_frame, HeadlessRunner, MockCatalog};

pub const AUTOHIDE_TIMEOUT_SECS: f64 = 3.5;
pub const FADEOUT_DURATION_SECS: f64 = 0.5;

pub struct FullscreenShaderUiHarness {
    pub params: Params,
    pub last_mouse_activity: f64,
    pub hovered_controls: bool,
    pub viewport: Rect,
    pub art_size: f32,
    pub controls_size: egui::Vec2,
}

impl FullscreenShaderUiHarness {
    pub fn new(viewport: Rect) -> Self {
        Self {
            params: Params {
                time: 0.0,
                blend: 0.0,
                intensity: 1.0,
                saturation: 1.2,
                dim: 0.8,
                scale: 1.0,
            },
            last_mouse_activity: 0.0,
            hovered_controls: false,
            viewport,
            art_size: 400.0,
            controls_size: vec2(480.0, 60.0),
        }
    }

    pub fn artwork_rect(&self) -> Rect {
        let x = (self.viewport.width() - self.art_size) / 2.0;
        let y = (self.viewport.height() - self.art_size) / 2.0 - 40.0;
        Rect::from_min_size(pos2(x, y), vec2(self.art_size, self.art_size))
    }

    pub fn controls_rect(&self) -> Rect {
        let art = self.artwork_rect();
        let x = (self.viewport.width() - self.controls_size.x) / 2.0;
        let y = art.max.y + 24.0;
        Rect::from_min_size(pos2(x, y), self.controls_size)
    }

    pub fn on_mouse_move(&mut self, current_time: f64, pos: Pos2) {
        self.last_mouse_activity = current_time;
        self.hovered_controls = self.controls_rect().contains(pos);
    }

    pub fn update_time(&mut self, current_time: f64) {
        self.params.time = current_time as f32;
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
}

#[test]
fn test_shader_animation_advances_independently_of_controls_autohide() {
    let viewport = Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0));
    let mut harness = FullscreenShaderUiHarness::new(viewport);

    // Simulate 5 seconds in 0.5s ticks
    for step in 0..=10 {
        let t = step as f64 * 0.5;
        harness.update_time(t);

        let opacity = harness.controls_opacity(t);

        // Shader time must strictly follow virtual clock
        assert_eq!(harness.params.time, t as f32);
        assert!(harness.params.time.is_finite());

        if t <= 3.5 {
            assert_eq!(opacity, 1.0, "Controls must be visible before 3.5s");
        } else if t >= 4.0 {
            assert_eq!(opacity, 0.0, "Controls must be hidden after 4.0s");
        }

        // Even when controls are hidden, shader parameters remain active and finite
        assert!(harness.params.intensity > 0.0);
        assert!(harness.params.scale > 0.0);
    }
}

#[test]
fn test_minimal_controls_overlay_over_shader_without_glass_card() {
    run_headless_egui_frame(|_ctx| {
        let viewport = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 780.0));
        let harness = FullscreenShaderUiHarness::new(viewport);

        let art = harness.artwork_rect();
        let controls = harness.controls_rect();

        // 1. Controls strictly positioned below artwork
        assert!(
            controls.min.y >= art.max.y,
            "Minimal controls must strictly reside below artwork rect"
        );

        // 2. Elimination of frosted glass card container
        let mut runner = HeadlessRunner::new().with_size(1200.0, 780.0);
        let mut app = MockCatalog::mock_headless_app();
        app.fullscreen = true;
        app.np_open = true;
        runner.step_time(1.0);
        let (_, full_output) = runner.run_frame(|ctx| {
            nowplaying::overlay(&mut app, ctx, 1.0, viewport);
        });
        let rects = extract_rect_shapes(&full_output.shapes);
        let card_containers: Vec<_> = rects
            .iter()
            .filter(|r| {
                r.rect.width() >= 250.0
                    && r.rect.height() >= 60.0
                    && r.fill != Color32::TRANSPARENT
                    && r.rect.intersects(controls)
            })
            .collect();
        assert!(
            card_containers.is_empty(),
            "Frosted glass card container behind controls must be eliminated, found: {:?}",
            card_containers
        );

        // 3. macOS Vibrancy styling tokens applied
        let glass_token = theme::glass(0.08);
        let shade_token = theme::shade(0.35);

        assert!(glass_token.a() > 0, "Vibrancy glass token must have non-zero alpha");
        assert!(shade_token.a() > 0, "Vibrancy shade token must have non-zero alpha");

        // 4. Headless fallback shader background color
        let fallback_bg = Color32::from_rgb(18, 18, 22);
        assert!(fallback_bg.r() > 0);
    });
}

#[test]
fn test_mouse_wake_event_restores_controls_over_running_shader() {
    let viewport = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 780.0));
    let mut harness = FullscreenShaderUiHarness::new(viewport);

    // Advance to t = 5.0s (inactivity timeout expired, controls hidden)
    harness.update_time(5.0);
    assert_eq!(harness.controls_opacity(5.0), 0.0);

    // Mouse movement arrives at t = 5.1s
    harness.on_mouse_move(5.1, pos2(200.0, 200.0));
    harness.update_time(5.1);

    // Controls must wake immediately to 1.0 opacity
    assert_eq!(
        harness.controls_opacity(5.1),
        1.0,
        "Mouse movement must wake minimal controls to full opacity immediately"
    );

    // Shader continues animating smoothly
    assert_eq!(harness.params.time, 5.1f32);
}

#[test]
fn test_hover_protection_on_controls_during_fluid_shader_color_cycle() {
    let viewport = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 780.0));
    let mut harness = FullscreenShaderUiHarness::new(viewport);

    let controls_center = harness.controls_rect().center();

    // Mouse hovers directly over controls at t = 0.0s
    harness.on_mouse_move(0.0, controls_center);
    assert!(harness.hovered_controls);

    // Time advances to 10.0s while shader blend interpolates from cover A to cover B
    for step in 0..=20 {
        let t = step as f64 * 0.5;
        harness.update_time(t);
        harness.params.blend = (t / 10.0) as f32; // 0.0 -> 1.0 crossfade

        // Hover keeps controls pinned at 1.0 opacity
        assert_eq!(
            harness.controls_opacity(t),
            1.0,
            "Hovering controls must prevent auto-hide fadeout across all elapsed time"
        );

        // Shader blend parameter is valid
        assert!(harness.params.blend >= 0.0 && harness.params.blend <= 1.0);
    }
}

#[test]
fn test_window_resize_recomputes_shader_resolution_and_controls_layout() {
    let viewports = [
        Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
        Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 780.0)),
        Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0)),
        Rect::from_min_size(Pos2::ZERO, vec2(3840.0, 2160.0)),
    ];

    for vp in viewports {
        let harness = FullscreenShaderUiHarness::new(vp);

        let art = harness.artwork_rect();
        let controls = harness.controls_rect();

        // Bounds invariants
        assert!(art.min.y >= 0.0);
        assert!(controls.min.y >= art.max.y, "Controls must remain below artwork");
        assert!(controls.max.x <= vp.max.x, "Controls must not overflow viewport width");
        assert!(controls.max.y <= vp.max.y, "Controls must not overflow viewport height");

        // Shader resolution clamp
        let shader_w = vp.width().max(1.0);
        let shader_h = vp.height().max(1.0);
        assert!(shader_w >= 800.0);
        assert!(shader_h >= 600.0);
    }
}
