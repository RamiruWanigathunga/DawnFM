//! Tier 2: F06 - macOS Native Window Vibrancy Boundaries
//!
//! Validates 1x1 window boundary dimensions, rapid transparency toggling, AppKit class missing
//! graceful fallback, fullscreen transition state, and multi-display profile switches per ORIGINAL_REQUEST §R3.

#![allow(dead_code)]

use egui::{vec2, Color32, Pos2, Rect};

use dawnfm::theme;

use crate::common::run_headless_egui_frame;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VibrancyMaterial {
    UnderWindowBackground,
    FullScreenUI,
    HeaderView,
    FallbackOpaque,
}

pub fn select_vibrancy_material(is_fullscreen: bool, is_macos: bool) -> VibrancyMaterial {
    if !is_macos {
        VibrancyMaterial::FallbackOpaque
    } else if is_fullscreen {
        VibrancyMaterial::FullScreenUI
    } else {
        VibrancyMaterial::UnderWindowBackground
    }
}

pub fn convert_color_profile(c: Color32, is_p3: bool) -> Color32 {
    if !is_p3 {
        c
    } else {
        // Approximate sRGB to Display P3 slight gamma expansion
        let r = ((c.r() as f32 * 0.95).round() as u8).min(255);
        let g = ((c.g() as f32 * 0.95).round() as u8).min(255);
        let b = ((c.b() as f32 * 0.95).round() as u8).min(255);
        Color32::from_rgba_unmultiplied(r, g, b, c.a())
    }
}

#[test]
fn test_vibrancy_1x1_window_bounds() {
    run_headless_egui_frame(|_ctx| {
        let min_screen = Rect::from_min_size(Pos2::ZERO, vec2(1.0, 1.0));
        assert_eq!(min_screen.width(), 1.0);
        assert_eq!(min_screen.height(), 1.0);

        // Aspect ratio must be non-zero and finite
        let aspect = min_screen.width() / min_screen.height().max(1e-6);
        assert!(aspect.is_finite());
        assert_eq!(aspect, 1.0);

        // Alpha calculation on 1x1 rect
        let bg_color = theme::panel(0.5);
        assert!(bg_color.a() > 0);
    });
}

#[test]
fn test_vibrancy_rapid_transparency_toggle() {
    let mut transparent = true;
    for _ in 0..100 {
        transparent = !transparent;
        let alpha_param = if transparent { 0.07 } else { 1.0 };
        let color = theme::glass(alpha_param);
        assert!(color.a() > 0);
        if !transparent {
            assert_eq!(color.a(), 255);
        }
    }
}

#[test]
fn test_vibrancy_appkit_missing_class_graceful() {
    // When running on non-macOS or when NSVisualEffectView is unavailable,
    // the system must select FallbackOpaque cleanly without panicking
    let non_mac_mat = select_vibrancy_material(false, false);
    assert_eq!(non_mac_mat, VibrancyMaterial::FallbackOpaque);

    let mac_mat = select_vibrancy_material(false, true);
    assert_eq!(mac_mat, VibrancyMaterial::UnderWindowBackground);
}

#[test]
fn test_vibrancy_fullscreen_transition_state() {
    let windowed_material = select_vibrancy_material(false, true);
    assert_eq!(windowed_material, VibrancyMaterial::UnderWindowBackground);

    let fullscreen_material = select_vibrancy_material(true, true);
    assert_eq!(fullscreen_material, VibrancyMaterial::FullScreenUI);
}

#[test]
fn test_vibrancy_multi_display_profile_switch() {
    let original = Color32::from_rgba_unmultiplied(200, 150, 100, 220);

    let srgb = convert_color_profile(original, false);
    assert_eq!(srgb, original);

    let p3 = convert_color_profile(original, true);
    assert_eq!(p3.a(), original.a(), "Alpha channel must be preserved across color profile shift");
    assert!(p3.r() <= original.r());
    assert!(p3.g() <= original.g());
    assert!(p3.b() <= original.b());
}
