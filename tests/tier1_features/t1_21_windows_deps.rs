//! Tier 1: F21 - Windows Cargo Dependencies Partition
//!
//! Validates `[target.'cfg(windows)'.dependencies]` isolation, `windows` crate features,
//! keyring native backend partitioning, macOS isolation, and zero overhead on macOS per ORIGINAL_REQUEST §R6.

#[test]
fn test_cargo_toml_windows_target_deps_section() {
    let cargo_toml = std::fs::read_to_string("Cargo.toml")
        .expect("Dawn FM Cargo.toml must exist and be readable");

    assert!(
        cargo_toml.contains("[target.'cfg(windows)'.dependencies]"),
        "Dawn FM Cargo.toml must isolate Windows dependencies under [target.'cfg(windows)'.dependencies]"
    );
}

#[test]
fn test_cargo_toml_windows_crate_present() {
    let cargo_toml = std::fs::read_to_string("Cargo.toml")
        .expect("Dawn FM Cargo.toml must exist and be readable");

    assert!(
        cargo_toml.contains("windows = {"),
        "Windows target dependencies in Cargo.toml must include the 'windows' crate"
    );
    assert!(
        cargo_toml.contains("Win32_Graphics_Dwm"),
        "Windows crate features must declare Win32_Graphics_Dwm for DWM shadow frame extension"
    );
    assert!(
        cargo_toml.contains("Win32_UI_WindowsAndMessaging"),
        "Windows crate features must declare Win32_UI_WindowsAndMessaging for subclassing"
    );
}

#[test]
fn test_cargo_toml_keyring_windows_native() {
    let cargo_toml = std::fs::read_to_string("Cargo.toml")
        .expect("Dawn FM Cargo.toml must exist and be readable");

    assert!(
        cargo_toml.contains("features = [\"windows-native\"]"),
        "keyring on Windows target in Cargo.toml must enable windows-native feature"
    );
    assert!(
        cargo_toml.contains("features = [\"apple-native\"]"),
        "keyring on macOS target in Cargo.toml must use apple-native feature"
    );
}

#[test]
fn test_cargo_toml_macos_dependencies_isolated() {
    let cargo_toml = std::fs::read_to_string("Cargo.toml")
        .expect("Dawn FM Cargo.toml must exist and be readable");

    assert!(
        cargo_toml.contains("objc2 ="),
        "Dawn FM Cargo.toml must declare objc2 for macOS"
    );
    assert!(
        cargo_toml.contains("objc2-foundation ="),
        "Dawn FM Cargo.toml must declare objc2-foundation for macOS"
    );
}

#[test]
fn test_cargo_toml_zero_macos_overhead() {
    let cargo_toml = std::fs::read_to_string("Cargo.toml")
        .expect("Dawn FM Cargo.toml must exist and be readable");
    let deps_section = cargo_toml.split("[dependencies]").nth(1).unwrap_or("");
    let general_deps = deps_section.split("\n[").next().unwrap_or("");

    assert!(
        !general_deps.contains("windows ="),
        "General [dependencies] must not include un-guarded 'windows' crate"
    );
    assert!(
        !general_deps.contains("windows-sys ="),
        "General [dependencies] must not include un-guarded 'windows-sys' crate"
    );
}
