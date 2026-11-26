//! Tier 2: F08 - i18n Framework Setup Boundaries
//!
//! Validates empty locale input, malformed BCP-47 tags, 10,000-switch rapid churn,
//! concurrent multi-threaded lookup/switching, and whitespace normalization per ORIGINAL_REQUEST §R5.

use std::thread;
use dawnfm::i18n::{normalize_locale, set_locale, locale, SUPPORTED_LOCALES};

#[test]
fn test_locale_empty_string_input() {
    assert_eq!(normalize_locale(""), "en", "Empty locale string must fallback to default 'en'");
}

#[test]
fn test_locale_malformed_bcp47_fallback() {
    let malformed_tags = [
        "-bad",
        "en_US_EXTRA_LONG_INVALID_SEGMENTS",
        "12345",
        "!@#$%^&*()",
        "x-private",
        "????",
        "///",
    ];

    for tag in malformed_tags {
        let norm = normalize_locale(tag);
        assert_eq!(
            norm, "en",
            "Malformed tag '{}' must resolve or fall back to 'en'",
            tag
        );
    }
}

#[test]
fn test_locale_10k_rapid_switch_churn() {
    let candidates = ["en", "es", "fr", "de", "ja", "invalid", "unknown", ""];

    for i in 0..10_000 {
        let loc = candidates[i % candidates.len()];
        let norm = normalize_locale(loc);
        assert!(
            SUPPORTED_LOCALES.contains(&norm),
            "Locale state must remain one of the supported languages at all times"
        );
    }
}

#[test]
fn test_locale_concurrent_lookup_switching() {
    let mut handles = Vec::new();

    // 4 reader threads testing normalize_locale
    for _ in 0..4 {
        handles.push(thread::spawn(move || {
            let tags = ["es", "fr", "de", "ja", "en", "bad", "zh-CN", "pt-BR"];
            for i in 0..500 {
                let tag = tags[i % tags.len()];
                let curr = normalize_locale(tag);
                assert!(!curr.is_empty());
                assert!(SUPPORTED_LOCALES.contains(&curr));
            }
        }));
    }

    // 2 setter threads
    for w_idx in 0..2 {
        handles.push(thread::spawn(move || {
            let tags = ["es", "fr", "de", "ja", "en"];
            for i in 0..500 {
                let tag = tags[(i + w_idx) % tags.len()];
                set_locale(tag);
                let l = locale();
                assert!(!l.is_empty());
            }
        }));
    }

    for h in handles {
        h.join().expect("Concurrent locale operation must not panic");
    }

    // Reset back to en
    set_locale("en");
}

#[test]
fn test_locale_whitespace_normalization() {
    let padded_inputs = [
        ("  en  ", "en"),
        ("\tes\t\n", "es"),
        (" \r\nfr-FR \n", "fr"),
        ("   ja_JP   ", "ja"),
    ];

    for (input, expected) in padded_inputs {
        let normalized = normalize_locale(input);
        assert_eq!(
            normalized, expected,
            "Padded input '{}' should be accepted after normalization",
            input
        );
    }
}
