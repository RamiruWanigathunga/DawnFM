//! Tier 1: F09 - UI String Localization
//!
//! Validates `t!()` translation key lookup, missing key fallback,
//! and physical dictionary file verification across supported locales.

use dawnfm::t;

#[test]
fn test_t_macro_basic_keys() {
    assert_eq!(t!("home", locale = "en"), "Home");
    assert_eq!(t!("search", locale = "en"), "Search");
    assert_eq!(t!("play", locale = "en"), "Play");
    assert_eq!(t!("pause", locale = "en"), "Pause");
    assert_eq!(t!("toggle_queue", locale = "en"), "Toggle queue");
    assert_eq!(t!("repeat", locale = "en"), "Repeat");
    assert_eq!(t!("shuffle", locale = "en"), "Shuffle");
}

#[test]
fn test_t_macro_missing_key_fallback() {
    let missing_key = "non_existent_button_label";
    assert_eq!(
        t!(missing_key, locale = "en"),
        missing_key,
        "Missing translation key must gracefully fall back to the key itself without panic"
    );
}

#[test]
fn test_locales_json_valid_syntax() {
    for loc in &["en", "fr", "es", "de", "ja"] {
        let path = format!("locales/{loc}.json");
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Physical {} must exist: {}", path, e));
        let physical_parsed: Result<serde_json::Value, _> = serde_json::from_str(&content);
        assert!(physical_parsed.is_ok(), "Physical {} must be valid JSON", path);
    }
}

#[test]
fn test_locales_no_empty_values() {
    for loc in &["en", "fr", "es", "de", "ja"] {
        let path = format!("locales/{loc}.json");
        let content = std::fs::read_to_string(&path).expect("Read locale file");
        let parsed: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&content).expect("Parse locale JSON");
        for (k, v) in &parsed {
            if let Some(s) = v.as_str() {
                assert!(!s.trim().is_empty(), "Translation value for '{}' in {} cannot be empty", k, loc);
            }
        }
    }
}
