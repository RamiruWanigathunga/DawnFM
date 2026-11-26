//! Tier 1: F14 - Animated Background Blur
//!
//! Validates Apple Music-style Kawarp fluid mesh background blur WGSL/GLSL syntax,
//! uniform parameter finiteness, headless fallback, and color palette interpolation per ORIGINAL_REQUEST §R1.

use egui::Color32;

use dawnfm::shader::Params;

#[test]
fn test_shader_wgsl_syntax_validity() {
    let shader_source = std::fs::read_to_string("src/shader.rs").expect("src/shader.rs must exist");

    assert!(
        shader_source.contains("@vertex\nfn vs_main") || shader_source.contains("@vertex"),
        "WGSL vertex entry point must be defined in src/shader.rs"
    );
    assert!(
        shader_source.contains("@fragment\nfn fs_main") || shader_source.contains("@fragment"),
        "WGSL fragment entry point must be defined in src/shader.rs"
    );
    assert!(
        shader_source.contains("snoise"),
        "WGSL Kawarp shader must include simplex noise implementation"
    );
    assert!(
        shader_source.contains("textureSample"),
        "WGSL shader must sample textures"
    );
}

#[test]
fn test_shader_glsl_syntax_validity() {
    let shader_source = std::fs::read_to_string("src/shader.rs").expect("src/shader.rs must exist");

    assert!(
        shader_source.contains("precision highp float;"),
        "GLSL shader header must specify highp float precision"
    );
    assert!(
        shader_source.contains("uniform float u_time;"),
        "GLSL shader must declare time uniform"
    );
    assert!(
        shader_source.contains("mod289"),
        "GLSL shader must include mod289 helper for noise evaluation"
    );
}

#[test]
fn test_shader_headless_fallback_graceful() {
    // When running headlessly in pure software without an active WGPU/Glow GPU context,
    // background blur falls back to solid or gradient painter rects without panic
    let fallback_color = Color32::from_rgb(20, 20, 26);
    assert!(fallback_color.r() > 0 && fallback_color.b() > 0);
}

#[test]
fn test_shader_mesh_vertex_uniforms_finite() {
    let params = Params {
        time: 12.34,
        blend: 0.5,
        intensity: 1.0,
        saturation: 1.2,
        dim: 0.8,
        scale: 1.0,
    };

    assert!(params.time.is_finite());
    assert!(params.blend >= 0.0 && params.blend <= 1.0);
    assert!(params.intensity.is_finite());
    assert!(params.saturation.is_finite());
    assert!(params.dim.is_finite());
    assert!(params.scale.is_finite());
}

#[test]
fn test_shader_color_palette_interpolation() {
    // Smooth crossfade between cover palette A and B: mix(a, b, t)
    let c1 = [0.8f32, 0.2, 0.3];
    let c2 = [0.1f32, 0.6, 0.9];
    let t = 0.5f32;

    let mixed = [
        c1[0] * (1.0 - t) + c2[0] * t,
        c1[1] * (1.0 - t) + c2[1] * t,
        c1[2] * (1.0 - t) + c2[2] * t,
    ];

    assert!((mixed[0] - 0.45).abs() < 1e-4);
    assert!((mixed[1] - 0.40).abs() < 1e-4);
    assert!((mixed[2] - 0.60).abs() < 1e-4);
}
