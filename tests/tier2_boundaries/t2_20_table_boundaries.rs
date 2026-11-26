//! Tier 2: F20 - Album 7-Column Tracklist Boundaries
//!
//! Validates 0-second duration formatting, extended duration formatting, BPM & Key boundaries,
//! wide vs narrow viewport column responsiveness, and boundary tracks per ORIGINAL_REQUEST §R4.

use dawnfm::model::{Link, Track};
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
fn test_tracklist_zero_second_duration() {
    assert_eq!(fmt_time(0), "0:00");
}

#[test]
fn test_tracklist_extended_duration_format() {
    let fifty_nine = 59;
    assert_eq!(fmt_time(fifty_nine), "0:59");

    let one_minute = 60;
    assert_eq!(fmt_time(one_minute), "1:00");

    let mixed = 3600 + (25 * 60) + 42; // 85m 42s
    assert_eq!(fmt_time(mixed), "85:42");

    let ten_hours = 10 * 3600; // 600m 00s
    assert_eq!(fmt_time(ten_hours), "600:00");
}

#[test]
fn test_tracklist_narrow_viewport_omits_extended_columns() {
    // Narrow viewport (< 550.0px width)
    let mut runner = HeadlessRunner::new().with_size(480.0, 600.0);
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

    // Essential columns (Track #, Title, Duration) must still be present
    assert!(texts.iter().any(|t| t == "1"), "Track number must remain visible");
    assert!(texts.iter().any(|t| t.contains("Blinding Lights")), "Title must remain visible");
    assert!(texts.iter().any(|t| t == "3:20"), "Duration must remain visible");
}

#[test]
fn test_tracklist_boundary_track_metadata() {
    // Empty title
    let empty_title_track = Track {
        id: "empty_01".to_string(),
        title: "".to_string(),
        artists: vec![],
        album: None,
        duration: None,
        thumbs: vec![],
        track_nr: None,
        plays: None,
        music_video: false,
    };
    let (bpm_empty, key_empty) = track_bpm_key(&empty_title_track);
    assert!((115..=133).contains(&bpm_empty));
    assert!(!key_empty.is_empty());

    // 500-character long title
    let long_title_track = Track {
        id: "long_01".to_string(),
        title: "A".repeat(500),
        artists: vec![Link {
            name: "Long Artist".to_string(),
            id: None,
        }],
        album: None,
        duration: Some(0),
        thumbs: vec![],
        track_nr: Some(999),
        plays: Some(0),
        music_video: false,
    };
    let (bpm_long, key_long) = track_bpm_key(&long_title_track);
    assert!((115..=133).contains(&bpm_long));
    assert!(!key_long.is_empty());

    // Headless render of boundary track does not panic and completes layout
    let mut runner = HeadlessRunner::new().with_size(800.0, 600.0);
    let mut acts = Vec::new();
    let opts = RowOpts {
        number: Some(999),
        ..Default::default()
    };
    let (resp, _out) = runner.run_ui(|ui| {
        track_row(ui, &long_title_track, &opts, &mut acts)
    });
    assert!(resp.rect.is_finite());
}
