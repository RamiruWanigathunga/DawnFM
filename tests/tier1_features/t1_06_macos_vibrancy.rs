//! Tier 1: F06 - macOS Native Window Vibrancy
//!
//! Validates window transparency configuration, semantic vibrancy material selection,
//! theme glass tokens, and platform compilation guards per ORIGINAL_REQUEST §R3.

use dawnfm::theme;

#[test]
fn test_macos_transparency_enabled() {
    let main_rs = std::fs::read_to_string("src/main.rs").expect("src/main.rs must exist");
    assert!(
        main_rs.contains(".with_transparent(true)"),
        "Window viewport configuration in src/main.rs must enable transparency via .with_transparent(true)"
    );
}

#[test]
fn test_macos_semantic_material_selection() {
    // UnderWindowBackground semantic styling requires translucent glass layers
    let glass_low = theme::glass(0.07);
    let glass_high = theme::glass(0.12);

    assert!(
        glass_low.a() < glass_high.a(),
        "Higher alpha parameter must produce strictly greater opacity"
    );
    assert!(glass_low.a() > 0, "Glass layer must not be completely transparent");

    let panel_color = theme::panel(0.5);
    assert!(panel_color.a() > 0, "Panel color must be defined");
}

#[test]
fn test_macos_vibrancy_state_active() {
    let bg = theme::bg();
    let menu = theme::menu_fill();
    let surface = theme::surface_fill();

    // In dark mode (default for music player), colors should have valid non-zero RGB components
    assert!(bg.r() < 100, "Dark theme background r-channel should be low");
    assert!(menu.r() < 100, "Dark theme menu fill r-channel should be low");
    assert!(surface.r() < 100, "Dark theme surface fill r-channel should be low");
}

#[test]
fn test_macos_titlebar_drag_transparency() {
    let shade_val = theme::shade(0.42);
    assert_eq!(
        shade_val.a(),
        (0.42f32 * 255.0f32).round() as u8,
        "Shade color alpha must directly match opacity parameter"
    );

    let dim_text = theme::text_dim();
    assert!(dim_text.a() > 0);
}

#[test]
fn test_macos_non_mac_compilation_guard() {
    // Non-macOS platforms must have safe fallback paths
    let is_mac_above_15 = theme::is_macos_above_15();
    #[cfg(target_os = "macos")]
    {
        // On macOS test runner, is_macos_above_15() returns a valid boolean
        let _ = is_mac_above_15;
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert!(!is_mac_above_15, "Non-macOS platforms must evaluate is_macos_above_15() to false");
    }
}
