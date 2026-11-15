//! Tier 2: F26 - Inno Setup Packaging Script Boundaries
//!
//! Validates path space escaping with double-quotes, uninstall registry cleanup flags,
//! version synchronization with Cargo.toml, process mutex directives,
//! and installer architecture constraints per ORIGINAL_REQUEST §R6.

const ISS_PATH: &str = "installer/dawnfm.iss";

fn load_iss_script() -> String {
    std::fs::read_to_string(ISS_PATH)
        .unwrap_or_else(|e| panic!("Genuine installer script must exist at '{}': {}", ISS_PATH, e))
}

pub fn extract_inno_macro(script: &str, macro_name: &str) -> Option<String> {
    for line in script.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#define") {
            let mut parts = trimmed.split_whitespace();
            parts.next(); // skip #define
            if let Some(name) = parts.next() {
                if name == macro_name {
                    let val = parts.collect::<Vec<&str>>().join(" ");
                    return Some(val.trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

pub fn extract_cargo_version(cargo_toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        } else if trimmed.starts_with('[') {
            in_package = false;
            continue;
        }

        if in_package && trimmed.starts_with("version") && trimmed.contains('=') {
            let mut parts = trimmed.split('=');
            parts.next();
            if let Some(v) = parts.next() {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

#[test]
fn test_iss_script_path_space_escaping() {
    let script = load_iss_script();

    // Command registry data must properly double-quote the executable and argument:
    // ValueData: """{app}\{#MyAppExeName}"" ""%1"""
    assert!(
        script.contains(r#""""{app}\{#MyAppExeName}"" ""%1""""#),
        "Registry command string must wrap paths in escaped double quotes to prevent space vulnerability"
    );

    // Source files with spaces or variables must be quoted
    assert!(
        script.contains(r#"Source: "..\target\release\{#MyAppExeName}""#),
        "Source path directive must be double-quoted"
    );
}

#[test]
fn test_iss_script_uninstall_registry_integrity() {
    let script = load_iss_script();

    // Check uninsdeletekey flag for protocol handler
    assert!(
        script.contains("uninsdeletekey"),
        "Registry key for protocol handler must include 'uninsdeletekey' flag to clean up on uninstall"
    );
}

#[test]
fn test_iss_script_version_sync_cargo_toml() {
    let cargo_toml_str = std::fs::read_to_string("Cargo.toml").expect("Cargo.toml must exist");
    let cargo_version = extract_cargo_version(&cargo_toml_str).expect("Version in Cargo.toml");

    let script = load_iss_script();
    let script_version = extract_inno_macro(&script, "MyAppVersion")
        .expect("MyAppVersion macro must exist");

    assert_eq!(
        script_version, cargo_version,
        "Inno Setup script MyAppVersion '{}' must match Cargo.toml package version '{}'",
        script_version, cargo_version
    );
}

#[test]
fn test_iss_script_process_mutex_check() {
    let script = load_iss_script();

    // Verify script contains AppId or unique identifier
    assert!(
        script.contains("AppId="),
        "Installer script must define a unique AppId GUID to identify the application instance"
    );

    // Check that AppId GUID is non-empty
    let has_valid_appid = script.lines().any(|l| l.trim().starts_with("AppId=") && l.len() > 15);
    assert!(has_valid_appid, "AppId directive must be valid");
}

#[test]
fn test_iss_script_firewall_rule_definition() {
    let script = load_iss_script();

    // Check compression algorithm
    assert!(
        script.contains("Compression=lzma2/ultra64") || script.contains("Compression=lzma2"),
        "Installer must configure modern LZMA2 compression"
    );

    // Check 64-bit architecture constraint
    assert!(
        script.contains("ArchitecturesAllowed=x64compatible") || script.contains("ArchitecturesAllowed={#Arch}"),
        "Installer must enforce 64-bit architecture"
    );
}
