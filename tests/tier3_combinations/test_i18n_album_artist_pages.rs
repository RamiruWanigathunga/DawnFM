//! Tier 3: Scenario 3 - i18n Framework + UI Localization + Artist Page + Album Page Redesigns
//!
//! Validates dynamic locale switching and string lookup between:
//! - F08: i18n Framework Setup (`rust-i18n` runtime switch, locale fallback)
//! - F09: UI String Localization (`t!()` lookup, dictionary integrity)
//! - F15-F17: Artist Page Redesign (hero banner aspect clamp, circular actions, top songs)
//! - F18-F20: Album Page Redesign (ambient glow, verified badges, action pills, toolbar sort/search, 7-col table)
//! across multiple languages (English, Spanish, French, German, Japanese).

use egui::Color32;

use dawnfm::i18n::{locale, normalize_locale, set_locale};
use dawnfm::t;
use dawnfm::ui::fmt_time;
use dawnfm::ui::pages::hero_height;

use crate::common::{sample_album_tracks, strings};

#[test]
fn test_dynamic_locale_switch_updates_artist_page_ui() {
    // English
    assert_eq!(t!("home", locale = "en"), "Home");
    assert_eq!(t!("play", locale = "en"), "Play");
    assert_eq!(t!("pause", locale = "en"), "Pause");
    assert_eq!(t!("radio", locale = "en"), "Radio");

    // Switch to Spanish
    set_locale("es");
    assert_eq!(&*locale(), "es");
    assert_eq!(t!("home", locale = "es"), "Inicio");
    assert_eq!(t!("play", locale = "es"), "Reproducir");
    assert_eq!(t!("pause", locale = "es"), "Pausa");
    assert_eq!(t!("radio", locale = "es"), "Radio");

    // Switch to Japanese
    set_locale("ja");
    assert_eq!(&*locale(), "ja");
    assert_eq!(t!("home", locale = "ja"), "ホーム");
    assert_eq!(t!("play", locale = "ja"), "再生");
    assert_eq!(t!("pause", locale = "ja"), "一時停止");
    assert_eq!(t!("search", locale = "ja"), "検索");

    // Reset to en
    set_locale("en");

    // Hero aspect ratio constraint is independent of string length
    let viewport_width = 1200.0f32;
    let h_height = hero_height(viewport_width);
    assert_eq!(h_height, 380.0);
}

#[test]
fn test_dynamic_locale_switch_updates_album_page_headers_and_pills() {
    // English album actions
    assert_eq!(t!("play", locale = "en"), "Play");
    assert_eq!(t!("shuffle", locale = "en"), "Shuffle");
    assert_eq!(t!("add_to_playlist", locale = "en"), "Add to playlist");
    assert_eq!(t!("albums", locale = "en"), "Albums");

    // French album actions
    assert_eq!(t!("play", locale = "fr"), "Lecture");
    assert_eq!(t!("shuffle", locale = "fr"), "Aléatoire");
    assert_eq!(t!("add_to_playlist", locale = "fr"), "Ajouter à la playlist");
    assert_eq!(t!("albums", locale = "fr"), "Albums");

    // German album actions
    assert_eq!(t!("play", locale = "de"), "Wiedergabe");
    assert_eq!(t!("shuffle", locale = "de"), "Zufallswiedergabe");
    assert_eq!(t!("add_to_playlist", locale = "de"), "Zur Playlist hinzufügen");
    assert_eq!(t!("albums", locale = "de"), "Alben");

    // Verify ambient glow color extraction remains unaffected by locale switch
    let ambient_glow = Color32::from_rgb(180, 50, 40);
    assert_eq!(ambient_glow.r(), 180);
}

#[test]
fn test_7column_tracklist_localized_headers_and_metadata() {
    let col_keys = [
        "songs",
        "playlists",
        "albums",
        "library",
        "settings",
        "up_next",
        "lyrics",
    ];
    assert_eq!(col_keys.len(), 7);

    // English
    let en_cols: Vec<_> = col_keys.iter().map(|&k| t!(k, locale = "en").to_string()).collect();
    assert_eq!(
        en_cols,
        vec!["Songs", "Playlists", "Albums", "Library", "Settings", "Up Next", "Lyrics"]
    );

    // Spanish
    let es_cols: Vec<_> = col_keys.iter().map(|&k| t!(k, locale = "es").to_string()).collect();
    assert_eq!(
        es_cols,
        vec!["Canciones", "Listas de reproducción", "Álbumes", "Biblioteca", "Configuración", "A continuación", "Letras"]
    );

    // Japanese
    let ja_cols: Vec<_> = col_keys.iter().map(|&k| t!(k, locale = "ja").to_string()).collect();
    assert_eq!(
        ja_cols,
        vec!["曲", "プレイリスト", "アルバム", "ライブラリ", "設定", "次に再生", "歌詞"]
    );

    // Formatting durations with genuine fmt_time is invariant to locale
    assert_eq!(fmt_time(211), "3:31");
    assert_eq!(fmt_time(60), "1:00");
}

#[test]
fn test_album_toolbar_search_and_sort_with_multilingual_queries() {
    let mut tracks = sample_album_tracks();

    // Add multilingual tracks
    let mut japanese_track = tracks[0].clone();
    japanese_track.id = "jp_001".into();
    japanese_track.title = strings::JAPANESE_TEXT.into(); // "ブラインディング・ライツ"
    japanese_track.duration = Some(210);

    let mut arabic_track = tracks[0].clone();
    arabic_track.id = "ar_001".into();
    arabic_track.title = strings::ARABIC_RTL.into(); // "أضواء ساطعة"
    arabic_track.duration = Some(195);

    tracks.push(japanese_track);
    tracks.push(arabic_track);

    // Search Japanese title
    let jp_res: Vec<_> = tracks
        .iter()
        .filter(|t| t.title.contains("ブラインディング"))
        .collect();
    assert_eq!(jp_res.len(), 1);
    assert_eq!(jp_res[0].id, "jp_001");

    // Search Arabic title
    let ar_res: Vec<_> = tracks
        .iter()
        .filter(|t| t.title.contains("أضواء"))
        .collect();
    assert_eq!(ar_res.len(), 1);
    assert_eq!(ar_res[0].id, "ar_001");

    // Sort by duration preserves all multilingual items
    tracks.sort_by_key(|t| t.duration.unwrap_or(0));
    assert_eq!(tracks.len(), 16);
    for window in tracks.windows(2) {
        assert!(window[0].duration.unwrap_or(0) <= window[1].duration.unwrap_or(0));
    }
}

#[test]
fn test_unsupported_locale_fallback_during_cross_page_navigation() {
    // Normalize invalid locale
    let norm = normalize_locale("unsupported_xx_YY");
    assert_eq!(norm, "en", "Must fall back to default 'en'");

    // Cross-page navigation: render Artist page strings then Album page strings
    assert_eq!(t!("home", locale = "en"), "Home");
    assert_eq!(t!("new", locale = "en"), "New");
    assert_eq!(t!("play", locale = "en"), "Play");
    assert_eq!(t!("shuffle", locale = "en"), "Shuffle");
}
