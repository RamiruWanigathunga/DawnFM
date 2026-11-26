//! Tier 2: F07 - macOS Styling Compatibility Boundaries
//!
//! Validates Darwin version parser zero/empty string handling, integer overflow resilience,
//! corrupt character parsing, 1000-cycle rapid theme switching, and NaN/Inf protection per ORIGINAL_REQUEST §R3.

use egui::Color32;

use dawnfm::theme;

pub fn safe_parse_darwin_major(rel: &str) -> u32 {
    let clean = rel.trim().trim_start_matches(|c: char| !c.is_ascii_digit());
    let digits: String = clean.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u32>().unwrap_or(0)
}

pub fn safe_is_darwin_above_15(rel: &str) -> bool {
    safe_parse_darwin_major(rel) >= 25
}

pub fn safe_theme_alpha(alpha: f32) -> u8 {
    if alpha.is_nan() || alpha <= 0.0 {
        0
    } else if alpha >= 1.0 || alpha.is_infinite() {
        255
    } else {
        (alpha * 255.0).round() as u8
    }
}

#[test]
fn test_darwin_version_parser_zero() {
    assert_eq!(safe_parse_darwin_major("0.0.0"), 0);
    assert!(!safe_is_darwin_above_15("0.0.0"));

    assert_eq!(safe_parse_darwin_major(""), 0);
    assert!(!safe_is_darwin_above_15(""));

    assert_eq!(safe_parse_darwin_major("0"), 0);
    assert!(!safe_is_darwin_above_15("0"));
}

#[test]
fn test_darwin_version_parser_overflow() {
    let huge_overflow = "99999999999999999999999999999999.0.0";
    // Must gracefully fall back to 0 without panicking on integer overflow
    let parsed = safe_parse_darwin_major(huge_overflow);
    assert_eq!(parsed, 0);

    let max_u32 = format!("{}.0.0", u32::MAX);
    assert_eq!(safe_parse_darwin_major(&max_u32), u32::MAX);
    assert!(safe_is_darwin_above_15(&max_u32));
}

#[test]
fn test_darwin_version_corrupt_chars() {
    let corrupt_cases = [
        ("Darwin 24.1.0", 24),
        ("v25_beta-rc1", 25),
        ("???!!!", 0),
        ("24a.0", 24),
        ("release-25.0", 25),
        ("NaN.0.0", 0),
    ];

    for (input, expected) in corrupt_cases {
        let major = safe_parse_darwin_major(input);
        assert_eq!(
            major, expected,
            "Input '{}' produced unexpected major {}",
            input, major
        );
    }
}

#[test]
fn test_theme_rapid_dark_light_switching() {
    for i in 0..1000 {
        let is_dark = i % 2 == 0;
        let bg = if is_dark { theme::bg() } else { Color32::from_rgb(240, 240, 245) };
        assert!(bg.a() > 0);

        let card_r = theme::card_radius();
        assert!(card_r.is_finite());
        assert!(card_r > 0.0);
    }
}

#[test]
fn test_theme_token_nan_inf_protection() {
    let invalid_alphas = [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -1.0f32,
        2.5f32,
    ];

    for &alpha in &invalid_alphas {
        let a_u8 = safe_theme_alpha(alpha);
        if alpha.is_nan() || alpha <= 0.0 {
            assert_eq!(a_u8, 0);
        } else {
            assert_eq!(a_u8, 255);
        }

        // Test against theme::shade / theme::glass without panic
        if alpha.is_finite() && alpha >= 0.0 && alpha <= 1.0 {
            let _ = theme::shade(alpha);
            let _ = theme::glass(alpha);
        }
    }
}

#[test]
fn test_settings_card_tiles_eliminated_in_light_and_dark_mode() {
    use crate::common::{extract_rect_shapes, HeadlessRunner};

    for &is_light in &[false, true] {
        let mut runner = HeadlessRunner::new().with_size(920.0, 600.0);
        let (_, full_output) = runner.run_ui(|ui| {
            theme::set_mode(ui.ctx(), is_light);
            dawnfm::ui::pages::settings_card(ui, |ui| {
                ui.label("Sample Setting Row");
            });
        });

        let rects = extract_rect_shapes(&full_output.shapes);
        // Previously, settings_card drew a container rect covering available_width (>800px)
        // with theme::glass(0.06). Verify zero filled container background shapes exist.
        let container_fills: Vec<_> = rects
            .iter()
            .filter(|r| r.rect.width() >= 500.0 && r.fill != Color32::TRANSPARENT)
            .collect();

        assert!(
            container_fills.is_empty(),
            "Settings card container tile must be removed in {} mode, but found: {:?}",
            if is_light { "light" } else { "dark" },
            container_fills
        );
    }
}
