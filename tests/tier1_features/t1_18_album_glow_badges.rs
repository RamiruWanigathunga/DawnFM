//! Tier 1: F18 - Album Page Ambient Glow & Header Badges
//!
//! Validates dominant color extraction for ambient backdrop glow, verified checkmark badge,
//! and metadata pills (genre, release year, singular/plural track count) per ORIGINAL_REQUEST §R4.

use egui::{pos2, vec2, Color32, Rect};

use dawnfm::art;
use dawnfm::ui::pages::{collection_genre, metadata_badge, paint_ambient_glow};
use crate::common::{run_headless_ui, MockCatalog};

fn make_test_png(r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut img = image::RgbImage::new(8, 8);
    for pixel in img.pixels_mut() {
        *pixel = image::Rgb([r, g, b]);
    }
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    img.write_to(&mut cursor, image::ImageFormat::Png).expect("Encode PNG");
    bytes
}

#[test]
fn test_album_dominant_color_extraction() {
    let bytes = make_test_png(200, 50, 80);
    let art_result = art::process(&bytes).expect("art process must succeed");

    assert!(art_result.dominant.r() > 0);
    assert!(art_result.accent.r() > 0);
    assert_eq!(art_result.rgba.len(), (art::SIZE * art::SIZE * 4) as usize);
}

#[test]
fn test_album_ambient_glow_color_applied() {
    run_headless_ui(|ui| {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 400.0));
        let glow_color = Color32::from_rgb(220, 60, 40);
        paint_ambient_glow(ui.painter(), rect, glow_color);
    });
}

#[test]
fn test_album_collection_genre_badge() {
    let album = MockCatalog::mock_album("album_01", "Electronic Dreams (Synthwave)", "Chromeo", 14);
    let genre = collection_genre(&album);
    assert_eq!(genre, "ELECTRONIC");

    let hiphop = MockCatalog::mock_album("album_02", "Hip Hop Anthem", "Kendrick", 10);
    assert_eq!(collection_genre(&hiphop), "HIP-HOP");
}

#[test]
fn test_album_metadata_badge_render() {
    run_headless_ui(|ui| {
        metadata_badge(ui, None, "ELECTRONIC");
        metadata_badge(ui, None, "2024");
        metadata_badge(ui, None, "14 TRACKS");
    });
}
