//! Internationalization (i18n) infrastructure and locale detection for Dawn FM.

pub use rust_i18n::{available_locales, locale, set_locale};

/// Translates a key using the active runtime locale.
pub fn translate(key: &str) -> String {
    let loc = locale();
    let val = crate::_rust_i18n_translate(&*loc, key);
    let prefix = format!("{}.", &*loc);
    if let Some(stripped) = val.strip_prefix(&prefix) {
        stripped.to_string()
    } else {
        val.to_string()
    }
}

/// Translates a key using an explicit locale.
pub fn translate_locale(key: &str, loc: &str) -> String {
    let val = crate::_rust_i18n_translate(loc, key);
    let prefix = format!("{}.", loc);
    if let Some(stripped) = val.strip_prefix(&prefix) {
        stripped.to_string()
    } else {
        val.to_string()
    }
}

/// Macro for direct key lookup across crate boundaries.
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::translate($key)
    };
    ($key:expr, locale = $loc:expr) => {
        $crate::i18n::translate_locale($key, $loc)
    };
}

pub use t;

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
        // English
        assert_eq!(t!("home", locale = "en"), "Home");
        assert_eq!(t!("search", locale = "en"), "Search");
        assert_eq!(t!("play", locale = "en"), "Play");
        assert_eq!(t!("pause", locale = "en"), "Pause");
        assert_eq!(t!("settings", locale = "en"), "Settings");

        // French
        assert_eq!(t!("home", locale = "fr"), "Accueil");
        assert_eq!(t!("search", locale = "fr"), "Rechercher");
        assert_eq!(t!("play", locale = "fr"), "Lecture");
        assert_eq!(t!("pause", locale = "fr"), "Pause");
        assert_eq!(t!("settings", locale = "fr"), "Paramètres");

        // Spanish
        assert_eq!(t!("home", locale = "es"), "Inicio");
        assert_eq!(t!("search", locale = "es"), "Buscar");
        assert_eq!(t!("play", locale = "es"), "Reproducir");
        assert_eq!(t!("pause", locale = "es"), "Pausa");
        assert_eq!(t!("settings", locale = "es"), "Configuración");

        // German
        assert_eq!(t!("home", locale = "de"), "Startseite");
        assert_eq!(t!("search", locale = "de"), "Suchen");
        assert_eq!(t!("play", locale = "de"), "Wiedergabe");
        assert_eq!(t!("pause", locale = "de"), "Pause");
        assert_eq!(t!("settings", locale = "de"), "Einstellungen");

        // Japanese
        assert_eq!(t!("home", locale = "ja"), "ホーム");
        assert_eq!(t!("search", locale = "ja"), "検索");
        assert_eq!(t!("play", locale = "ja"), "再生");
        assert_eq!(t!("pause", locale = "ja"), "一時停止");
        assert_eq!(t!("settings", locale = "ja"), "設定");
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
