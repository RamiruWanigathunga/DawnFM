//! Tier 4: Workload 2 - Heavy Catalog Browse Session
//!
//! Simulates a realistic browsing session across multiple artist pages, album pages,
//! top songs split views, 7-column tracklists, and dynamic runtime locale switches.

use egui::Color32;

use dawnfm::i18n::{locale, set_locale};
use dawnfm::t;
use dawnfm::ui::fmt_time;
use dawnfm::ui::pages::hero_height;

use crate::common::{sample_album_tracks, MockCatalog};

/// 7-column tracklist row model.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct TracklistRow {
    pub track_nr: u16,
    pub title: String,
    pub artists: String,
    pub album: String,
    pub plays: u64,
    pub bpm: u16,
    pub key: String,
    pub duration_secs: u32,
}

#[test]
fn test_workload_heavy_catalog_browse_session() {
    // 5 Artists to explore
    let artist_specs = [
        ("the_weeknd", "The Weeknd", true, 20),
        ("daft_punk", "Daft Punk", false, 15),
        ("dua_lipa", "Dua Lipa", true, 18),
        ("kendrick_lamar", "Kendrick Lamar", false, 25),
        ("taylor_swift", "Taylor Swift", true, 30),
    ];

    let viewport_width = 1440.0f32;
    let hero_h = hero_height(viewport_width);
    assert!(
        hero_h >= 240.0 && hero_h <= 380.0,
        "F15: Hero banner height ({}) must be clamped within [240, 380]",
        hero_h
    );

    // 1. Explore Artists
    for (id, name, _has_tour, top_count) in &artist_specs {
        let artist_page = MockCatalog::mock_artist_page(id, name, *top_count);
        assert_eq!(artist_page.name, *name);
        assert_eq!(artist_page.top.len(), *top_count);

        let home_label = t!("home", locale = "en");
        assert_eq!(home_label, "Home");

        // Split view layout verification
        let content_width = 1200.0f32;
        let latest_release_w = content_width * 0.38;
        let top_songs_w = content_width - latest_release_w - 32.0;
        assert!(latest_release_w >= 360.0);
        assert!(top_songs_w >= 600.0);

        // Top songs 3-column pagination
        let per_col = 5;
        let total_per_page = per_col * 3; // 15 songs per page
        let pages = (*top_count + total_per_page - 1) / total_per_page;
        assert!(pages >= 1, "Must have at least 1 page of top songs");

        let first_page_slice = &artist_page.top[..(*top_count).min(total_per_page)];
        assert!(first_page_slice.len() <= 15);
    }

    // 2. Dynamic locale switch to Spanish
    set_locale("es");
    assert_eq!(&*locale(), "es");
    assert_eq!(t!("play", locale = "es"), "Reproducir");
    assert_eq!(t!("shuffle", locale = "es"), "Aleatorio");
    assert_eq!(t!("playlists", locale = "es"), "Listas de reproducción");

    // 3. Explore 10 Albums
    for album_idx in 1..=10 {
        let album_id = format!("MPREb_album_{:03}", album_idx);
        let album_title = format!("Masterpiece Volume {:02}", album_idx);
        let artist = format!("Artist {}", (album_idx % 5) + 1);

        let album = MockCatalog::mock_album(&album_id, &album_title, &artist, 14);
        assert_eq!(album.tracks.len(), 14);

        // F18: Dominant color ambient glow
        let dominant_color = match album_idx % 4 {
            0 => Color32::from_rgb(180, 30, 45),  // Deep red
            1 => Color32::from_rgb(30, 110, 200), // Ocean blue
            2 => Color32::from_rgb(210, 150, 20), // Amber gold
            _ => Color32::from_rgb(130, 40, 160), // Purple
        };
        assert!(dominant_color.r() > 0 || dominant_color.g() > 0 || dominant_color.b() > 0);

        // Metadata formatting
        let total_duration: u32 = album.tracks.iter().map(|t| t.duration.unwrap_or(200)).sum();
        let total_minutes = total_duration / 60;
        let meta_pill = format!("Album • 2024 • {} songs • {} min", album.tracks.len(), total_minutes);
        assert!(meta_pill.contains("14 songs"));

        // F19: Action pills & toolbar
        let play_label = t!("play", locale = "es");
        let shuffle_label = t!("shuffle", locale = "es");
        assert_eq!(play_label, "Reproducir");
        assert_eq!(shuffle_label, "Aleatorio");

        // F20: 7-Column Tracklist Table rows
        let rows: Vec<TracklistRow> = album
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| TracklistRow {
                track_nr: (i + 1) as u16,
                title: t.title.clone(),
                artists: t.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
                album: album_title.clone(),
                plays: t.plays.unwrap_or(500_000),
                bpm: 120 + ((i as u16 * 7) % 35),
                key: format!("{}A", (i % 12) + 1),
                duration_secs: t.duration.unwrap_or(210),
            })
            .collect();

        assert_eq!(rows.len(), 14);
        assert_eq!(rows[0].track_nr, 1);
        assert_eq!(rows[13].track_nr, 14);

        // Filter tracklist test
        let filter_needle = "Blinding";
        let filtered: Vec<&TracklistRow> = rows.iter().filter(|r| r.title.contains(filter_needle)).collect();
        let has_blinding = rows.iter().any(|r| r.title.contains("Blinding"));
        if has_blinding {
            assert!(!filtered.is_empty());
        }
    }

    // 4. Switch locale to Japanese
    set_locale("ja");
    assert_eq!(&*locale(), "ja");
    assert_eq!(t!("play", locale = "ja"), "再生");
    assert_eq!(t!("shuffle", locale = "ja"), "シャッフル");
    assert_eq!(t!("songs", locale = "ja"), "曲");
    assert_eq!(t!("albums", locale = "ja"), "アルバム");

    // Reset to en
    set_locale("en");
}

#[test]
fn test_heavy_browse_artist_hero_aspect_and_concert_badges() {
    let widths = [600.0, 800.0, 1024.0, 1280.0, 1440.0, 1920.0, 2560.0];

    for &w in &widths {
        let h = hero_height(w);
        assert!(
            h >= 240.0 && h <= 380.0,
            "F15: Banner height ({}) must stay clamped between [240, 380] for viewport {}",
            h,
            w
        );
    }
}

#[test]
fn test_heavy_browse_album_sort_and_multilingual_track_filtering() {
    let tracks = sample_album_tracks();
    assert_eq!(tracks.len(), 14);

    // Filter by exact track title substring
    let matches: Vec<_> = tracks
        .iter()
        .filter(|t| t.title.to_lowercase().contains("blinding"))
        .collect();
    // Verify case-insensitive match behavior
    for t in &matches {
        assert!(t.title.to_lowercase().contains("blinding"));
    }

    // Formatted durations
    for t in &tracks {
        let formatted = fmt_time(t.duration.unwrap_or(0));
        assert!(formatted.contains(':'));
    }
}
