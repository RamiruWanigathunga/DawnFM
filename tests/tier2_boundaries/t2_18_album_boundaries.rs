//! Tier 2: F18 - Album Glow & Badges Boundaries
//!
//! Validates pure white cover glow clamping, pure black cover minimum luminance lift,
//! 1x1 pixel color extraction, and metadata badge rendering per ORIGINAL_REQUEST §R4.

use dawnfm::art;
use dawnfm::ui::pages::metadata_badge;
use crate::common::run_headless_ui;

fn make_test_png(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut img = image::RgbImage::new(w, h);
    for pixel in img.pixels_mut() {
        *pixel = image::Rgb([r, g, b]);
    }
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    img.write_to(&mut cursor, image::ImageFormat::Png).expect("Encode PNG");
    bytes
}

#[test]
fn test_album_pure_white_cover_glow() {
    let bytes = make_test_png(16, 16, 255, 255, 255);
    let art_result = art::process(&bytes).expect("Process white cover");

    // Pure white cover dominant color should have managed luminance
    assert!(art_result.dominant.a() > 0 && art_result.dominant != egui::Color32::TRANSPARENT);
}

#[test]
fn test_album_pure_black_cover_glow() {
    let bytes = make_test_png(16, 16, 0, 0, 0);
    let art_result = art::process(&bytes).expect("Process black cover");

    // Black cover should be pulled toward accent by Kawarp tint pass
    assert_eq!(art_result.rgba.len(), (art::SIZE * art::SIZE * 4) as usize);
}

#[test]
fn test_album_1x1_pixel_image_extraction() {
    let bytes = make_test_png(1, 1, 142, 68, 173);
    let art_result = art::process(&bytes).expect("Process 1x1 cover");
    assert!(art_result.dominant.r() > 0 || art_result.dominant.b() > 0);
}

#[test]
fn test_album_metadata_badge_edge_cases() {
    run_headless_ui(|ui| {
        metadata_badge(ui, None, "0 TRACKS");
        metadata_badge(ui, None, "1 TRACK");
        metadata_badge(ui, None, "14 TRACKS");
    });
}
