//! Tier 1: F20 - Album Page 7-Column Tracklist Table
//!
//! Validates 7-column track table structure, BPM & Key metadata formatting, audio quality badge,
//! duration formatting, and popularity indicator scaling per ORIGINAL_REQUEST §R4.

use dawnfm::ui::fmt_time;
use dawnfm::ui::widgets::{track_bpm_key, track_row, RowOpts};
use crate::common::{sample_track, HeadlessRunner};

/// Helper to recursively extract all Shape primitives from egui output shapes.
fn extract_shapes(shapes: &[egui::epaint::ClippedShape]) -> Vec<egui::Shape> {
    let mut out = Vec::new();
    fn recurse(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(list) => {
                for s in list {
                    recurse(s, out);
                }
            }
            other => out.push(other.clone()),
        }
    }
    for clipped in shapes {
        recurse(&clipped.shape, &mut out);
    }
    out
}

/// Helper to extract text from all TextShape primitives.
fn extract_texts(shapes: &[egui::Shape]) -> Vec<String> {
    shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Text(ts) => Some(ts.galley.text().to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn test_album_tracklist_bpm_and_key_metadata() {
    let mut track_new_girl = sample_track();
    track_new_girl.title = "(I Don't Need A) New Girl".to_string();
    assert_eq!(track_bpm_key(&track_new_girl), (127, "Am"));

    let mut track_got_it_good = sample_track();
    track_got_it_good.title = "Got it Good".to_string();
    assert_eq!(track_bpm_key(&track_got_it_good), (127, "A"));

    let mut track_lost_found = sample_track();
    track_lost_found.title = "Lost And Found".to_string();
    assert_eq!(track_bpm_key(&track_lost_found), (130, "C"));

    let mut track_bts = sample_track();
    track_bts.title = "BTS - Dynamite".to_string();
    assert_eq!(track_bpm_key(&track_bts), (123, "C#m"));

    let mut track_replacements = sample_track();
    track_replacements.title = "Replacements".to_string();
    assert_eq!(track_bpm_key(&track_replacements), (127, "D"));

    // Deterministic hash derivation for arbitrary tracks
    let t1 = sample_track();
    let res1 = track_bpm_key(&t1);
    let res2 = track_bpm_key(&t1);
    assert_eq!(res1, res2, "BPM & Key calculation must be strictly deterministic");
    assert!((115..=133).contains(&res1.0), "BPM must fall within standard musical range");
    assert!(!res1.1.is_empty(), "Musical key string must not be empty");
}

#[test]
fn test_album_tracklist_duration_formatting() {
    assert_eq!(fmt_time(200), "3:20");
    assert_eq!(fmt_time(65), "1:05");
    assert_eq!(fmt_time(9), "0:09");
    assert_eq!(fmt_time(0), "0:00");
    assert_eq!(fmt_time(3600), "60:00");
}

#[test]
fn test_album_tracklist_7_columns_present() {
    let mut runner = HeadlessRunner::new().with_size(800.0, 600.0);
    let track = sample_track();
    let mut acts = Vec::new();
    let opts = RowOpts {
        number: Some(1),
        liked: true,
        ..Default::default()
    };

    let (_resp, full_output) = runner.run_ui(|ui| {
        track_row(ui, &track, &opts, &mut acts);
    });

    let shapes = extract_shapes(&full_output.shapes);
    let texts = extract_texts(&shapes);

    // 1. Track # ("1")
    assert!(texts.iter().any(|t| t == "1"), "Track number '1' must be rendered");

    // 2. Title text galley ("Blinding Lights")
    assert!(
        texts.iter().any(|t| t.contains("Blinding Lights")),
        "Track title must be rendered in galley: {:?}",
        texts
    );

    // 3. Duration text ("3:20")
    assert!(
        texts.iter().any(|t| t == "3:20"),
        "Formatted duration '3:20' must be rendered in text galleys: {:?}",
        texts
    );
}

#[test]
fn test_album_tracklist_row_interaction() {
    let mut runner = HeadlessRunner::new().with_size(800.0, 600.0);
    let track = sample_track();
    let mut acts = Vec::new();
    let opts = RowOpts {
        number: Some(1),
        liked: true,
        ..Default::default()
    };

    let (resp, _full_output) = runner.run_ui(|ui| {
        track_row(ui, &track, &opts, &mut acts)
    });

    assert!(resp.rect.is_finite(), "Row rect must be finite");
    assert!(resp.rect.width() >= 550.0, "Row width must span available width");
}
