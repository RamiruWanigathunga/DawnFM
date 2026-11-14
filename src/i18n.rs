//! Internationalization (i18n) infrastructure and locale detection for Dawn FM.

pub use rust_i18n::{available_locales, locale, set_locale, t};

/// Supported localization language codes.
pub const SUPPORTED_LOCALES: &[&str] = &["en", "fr", "es", "de", "ja"];

/// Parses and normalizes a raw locale string (e.g. "en-US", "fr_FR", "de-DE", "ja_JP")
/// to a supported 2-letter language code, falling back to "en".
pub fn normalize_locale(raw: &str) -> &'static str {
    let clean = raw.trim();
    if clean.is_empty() {
        return "en";
    }
    let lang = clean.split(&['-', '_'][..]).next().unwrap_or("en").to_lowercase();
    match lang.as_str() {
        "fr" => "fr",
        "es" => "es",
        "de" => "de",
        "ja" => "ja",
        _ => "en",
    }
}

/// Detects the system language at startup via `sys_locale::get_locale()`,
/// normalizes it with automatic fallback to "en", and configures the active locale.
pub fn detect_and_set_locale() -> String {
    let raw = sys_locale::get_locale().unwrap_or_else(|| "en".to_string());
    let effective = normalize_locale(&raw);
    rust_i18n::set_locale(effective);
    log::info!("i18n: detected system locale '{raw}', configured active locale '{effective}'");
    effective.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_locale_variants() {
        assert_eq!(normalize_locale("en-US"), "en");
        assert_eq!(normalize_locale("en_GB"), "en");
        assert_eq!(normalize_locale("en"), "en");
        assert_eq!(normalize_locale("fr-FR"), "fr");
        assert_eq!(normalize_locale("fr_CA"), "fr");
        assert_eq!(normalize_locale("fr"), "fr");
        assert_eq!(normalize_locale("es-ES"), "es");
        assert_eq!(normalize_locale("es_MX"), "es");
        assert_eq!(normalize_locale("de-DE"), "de");
        assert_eq!(normalize_locale("de_AT"), "de");
        assert_eq!(normalize_locale("ja-JP"), "ja");
        assert_eq!(normalize_locale("ja"), "ja");
        // Unsupported locales fallback to "en"
        assert_eq!(normalize_locale("zh-CN"), "en");
        assert_eq!(normalize_locale("pt-BR"), "en");
        assert_eq!(normalize_locale(""), "en");
        assert_eq!(normalize_locale("   "), "en");
        assert_eq!(normalize_locale("unknown"), "en");
    }

    #[test]
    fn test_detect_and_set_locale_does_not_panic() {
        let loc = detect_and_set_locale();
        assert!(SUPPORTED_LOCALES.contains(&loc.as_str()));
    }

    #[test]
    fn test_translations_across_all_supported_locales() {
        // English
        rust_i18n::set_locale("en");
        assert_eq!(t!("home"), "Home");
        assert_eq!(t!("search"), "Search");
        assert_eq!(t!("play"), "Play");
        assert_eq!(t!("pause"), "Pause");
        assert_eq!(t!("settings"), "Settings");

        // French
        rust_i18n::set_locale("fr");
        assert_eq!(t!("home"), "Accueil");
        assert_eq!(t!("search"), "Rechercher");
        assert_eq!(t!("play"), "Lecture");
        assert_eq!(t!("pause"), "Pause");
        assert_eq!(t!("settings"), "Paramètres");

        // Spanish
        rust_i18n::set_locale("es");
        assert_eq!(t!("home"), "Inicio");
        assert_eq!(t!("search"), "Buscar");
        assert_eq!(t!("play"), "Reproducir");
        assert_eq!(t!("pause"), "Pausa");
        assert_eq!(t!("settings"), "Configuración");

        // German
        rust_i18n::set_locale("de");
        assert_eq!(t!("home"), "Startseite");
        assert_eq!(t!("search"), "Suchen");
        assert_eq!(t!("play"), "Wiedergabe");
        assert_eq!(t!("pause"), "Pause");
        assert_eq!(t!("settings"), "Einstellungen");

        // Japanese
        rust_i18n::set_locale("ja");
        assert_eq!(t!("home"), "ホーム");
        assert_eq!(t!("search"), "検索");
        assert_eq!(t!("play"), "再生");
        assert_eq!(t!("pause"), "一時停止");
        assert_eq!(t!("settings"), "設定");

        // Restore English
        rust_i18n::set_locale("en");
    }

    #[test]
    fn test_missing_translation_key_falls_back_without_panic() {
        rust_i18n::set_locale("fr");
        // Completely non-existent key returns the key literal itself without panicking
        let non_existent = t!("completely_unknown_key_xyz");
        assert_eq!(non_existent, "completely_unknown_key_xyz");
        rust_i18n::set_locale("en");
    }
}
