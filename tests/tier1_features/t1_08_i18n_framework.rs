//! Tier 1: F08 - i18n Framework Setup
//!
//! Validates system locale detection, runtime language switching contract,
//! embedded dictionary structure, supported locales, and fallback to default "en"
//! per ORIGINAL_REQUEST §R5 and PROJECT.md § Interface Contracts.

use dawnfm::i18n::{detect_and_set_locale, locale, normalize_locale, set_locale, SUPPORTED_LOCALES};

#[test]
fn test_sys_locale_detection() {
    let detected = detect_and_set_locale();
    assert!(
        !detected.is_empty(),
        "Detected system locale string must not be empty"
    );
    assert!(
        SUPPORTED_LOCALES.contains(&detected.as_str()),
        "Detected locale should be supported or normalized: {}",
        detected
    );
}

#[test]
fn test_set_locale_runtime_switch() {
    set_locale("es");
    assert_eq!(&*locale(), "es");

    set_locale("fr");
    assert_eq!(&*locale(), "fr");

    set_locale("de");
    assert_eq!(&*locale(), "de");

    set_locale("ja");
    assert_eq!(&*locale(), "ja");

    set_locale("en");
    assert_eq!(&*locale(), "en");
}

#[test]
fn test_supported_locales_list() {
    assert_eq!(SUPPORTED_LOCALES, &["en", "fr", "es", "de", "ja"]);
    assert_eq!(normalize_locale("zh-CN"), "en"); // Normalizes unsupported to default "en"
    assert_eq!(normalize_locale("fr-FR"), "fr");
    assert_eq!(normalize_locale("es_MX"), "es");
    assert_eq!(normalize_locale("de_AT"), "de");
    assert_eq!(normalize_locale("ja-JP"), "ja");
}

#[test]
fn test_fallback_to_english() {
    assert_eq!(normalize_locale("pt-BR"), "en");
    assert_eq!(normalize_locale("ru-RU"), "en");
    assert_eq!(normalize_locale(""), "en");
    assert_eq!(normalize_locale("unknown"), "en");
}
