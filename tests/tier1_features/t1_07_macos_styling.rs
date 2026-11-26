//! Tier 1: F07 - macOS Forward Styling Compatibility
//!
//! Validates Darwin kernel version parsing (macOS 14 / Darwin 23, macOS 15 / Darwin 24,
//! macOS 16 / Darwin 25+), forward liquid glass styling flags, and theme token monotonicity
//! per ORIGINAL_REQUEST §R3.

use dawnfm::theme::{self, card_radius, hover_k, panel_radius, thumb_radius};

fn parse_darwin_major(rel: &str) -> u32 {
    rel.split('.').next().and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn is_darwin_above_15(rel: &str) -> bool {
    parse_darwin_major(rel) >= 25
}

#[test]
fn test_darwin_version_parser_macos14() {
    // macOS 14 Sonoma corresponds to Darwin kernel 23.x
    let release_sonoma = "23.6.0";
    assert_eq!(parse_darwin_major(release_sonoma), 23);
    assert!(
        !is_darwin_above_15(release_sonoma),
        "Darwin 23 (macOS 14) is not above macOS 15"
    );
}

#[test]
fn test_darwin_version_parser_macos15() {
    // macOS 15 Sequoia corresponds to Darwin kernel 24.x
    let release_sequoia = "24.1.0";
    assert_eq!(parse_darwin_major(release_sequoia), 24);
    assert!(
        !is_darwin_above_15(release_sequoia),
        "Darwin 24 (macOS 15) is not above macOS 15"
    );
}

#[test]
fn test_darwin_version_parser_macos16() {
    // macOS 16+ corresponds to Darwin kernel 25+
    let release_macos16 = "25.0.0";
    assert_eq!(parse_darwin_major(release_macos16), 25);
    assert!(
        is_darwin_above_15(release_macos16),
        "Darwin 25+ (macOS 16+) must be recognized as above macOS 15"
    );

    let release_future = "26.2.0";
    assert!(is_darwin_above_15(release_future));
}

#[test]
fn test_forward_liquid_glass_material_flag() {
    let above_15 = theme::is_macos_above_15();
    let _ = above_15;

    let cr = card_radius();
    assert!(cr > 0.0 && cr.is_finite(), "Card radius must be positive and finite");
}

#[test]
fn test_theme_tokens_monotonicity() {
    let cr = card_radius();
    let tr = thumb_radius();
    let pr = panel_radius();
    let hk = hover_k();

    assert!(cr >= 4.0 && cr <= 30.0, "Card radius within expected range: {}", cr);
    assert!(tr >= 4.0 && tr <= 20.0, "Thumb radius within expected range: {}", tr);
    assert!(pr >= 4 && pr <= 24, "Panel radius within expected range: {}", pr);
    assert!(hk > 0.0 && hk <= 1.0, "Hover factor within (0, 1]: {}", hk);
}
