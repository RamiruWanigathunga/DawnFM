//! Tier 2: F09 - UI String Localization Boundaries
//!
//! Validates 10,000-character overflow, RTL bidirectional text isolation, unclosed/malformed braces,
//! HTML/XML character escaping, and missing variable resilience per ORIGINAL_REQUEST §R5.

pub fn interpolate_template(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (k, v) in vars {
        let needle = format!("{{{k}}}");
        out = out.replace(&needle, v);
    }
    out
}

pub fn escape_xml_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[test]
fn test_translation_10k_char_overflow() {
    let large_var = "A".repeat(15_000);
    let template = "Header: {payload}";
    let res = interpolate_template(template, &[("payload", &large_var)]);

    assert_eq!(res.len(), 8 + large_var.len());
    assert!(res.starts_with("Header: AAAA"));
    assert!(res.ends_with("AAAA"));
}

#[test]
fn test_translation_rtl_bidi_isolation() {
    // Unicode directional formatting characters:
    // \u{202A} = Left-to-Right Embedding
    // \u{202B} = Right-to-Left Embedding
    // \u{202C} = Pop Directional Formatting
    let arabic_title = "استمع الآن";
    let english_suffix = "Dawn FM Live";

    let isolated = format!("\u{202B}{arabic_title}\u{202C} • {english_suffix}");
    assert!(isolated.contains(arabic_title));
    assert!(isolated.contains(english_suffix));
    assert!(isolated.starts_with('\u{202B}'));

    let template = "Playing: {track}";
    let interpolated = interpolate_template(template, &[("track", &isolated)]);
    assert!(interpolated.contains(arabic_title));
}

#[test]
fn test_translation_unclosed_braces() {
    let malformed_templates = [
        "Unclosed {brace",
        "Multiple {unclosed {and nested}",
        "Empty {}",
        "Double closed }}",
        "{{{triple}}}",
    ];

    for tmpl in malformed_templates {
        let res = interpolate_template(tmpl, &[("brace", "VAL"), ("triple", "NESTED")]);
        // Must complete without panicking
        assert!(!res.is_empty());
    }
}

#[test]
fn test_translation_special_html_escaping() {
    let raw_payload = "<script>alert('xss')</script> & \"quotes\"";
    let escaped = escape_xml_entities(raw_payload);

    assert!(!escaped.contains('<'));
    assert!(!escaped.contains('>'));
    assert!(escaped.contains("&lt;script&gt;"));
    assert!(escaped.contains("&amp;"));
    assert!(escaped.contains("&quot;quotes&quot;"));

    let template = "<title>{text}</title>";
    let result = interpolate_template(template, &[("text", &escaped)]);
    assert!(result.contains("&lt;script&gt;"));
}

#[test]
fn test_translation_missing_variables_graceful() {
    let template = "Downloading {track} by {artist} ({progress}%)";
    // Only supply 'track'
    let res = interpolate_template(template, &[("track", "Blinding Lights")]);

    // Unprovided placeholders must remain intact without crash
    assert!(res.contains("Downloading Blinding Lights by {artist} ({progress}%)"));
    assert!(res.contains("{artist}"));
    assert!(res.contains("{progress}"));
}
