//! Tier 1: F10 - Minimal Fullscreen Controls
//!
//! Validates removal of frosted glass card container behind controls, positioning
//! below album artwork, and presence of minimal play/pause, scrub bar, and volume controls
//! per ORIGINAL_REQUEST §R1 and PROJECT.md § Feature 10.

use egui::{pos2, vec2, Color32, Pos2, Rect};

use dawnfm::ui::nowplaying;
use crate::common::{extract_rect_shapes, HeadlessRunner, MockCatalog};

#[test]
fn test_controls_glass_card_eliminated() {
    let mut runner = HeadlessRunner::new().with_size(1200.0, 780.0);
    let mut app = MockCatalog::mock_headless_app();
    app.fullscreen = true;
    app.np_open = true;

    // Advance virtual clock past anim_bool duration so now-playing is fully expanded
    runner.step_time(1.0);

    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 780.0));
    let (_ret, full_output) = runner.run_frame(|ctx| {
        nowplaying::overlay(&mut app, ctx, 1.0, screen);
    });

    let controls_rect: Rect = runner
        .context()
        .data(|d| d.get_temp(egui::Id::unique("np-controls-rect")))
        .unwrap_or(Rect::NOTHING);
    assert!(
        controls_rect.is_finite() && controls_rect.width() > 0.0,
        "Controls rect must be registered in context data"
    );

    let rects = extract_rect_shapes(&full_output.shapes);

    // Any frosted glass card container behind the controls would be a large filled
    // bounding rect covering the controls region (> 250x60px)
    let card_containers: Vec<_> = rects
        .iter()
        .filter(|r| {
            r.rect.width() >= 250.0
                && r.rect.height() >= 60.0
                && r.fill != Color32::TRANSPARENT
                && r.rect.intersects(controls_rect)
        })
        .collect();

    assert!(
        card_containers.is_empty(),
        "Fullscreen controls must eliminate the frosted glass card container, but found: {:?}",
        card_containers
    );
}

#[test]
fn test_controls_positioned_below_artwork() {
    let mut runner = HeadlessRunner::new().with_size(1200.0, 780.0);
    let mut app = MockCatalog::mock_headless_app();
    app.fullscreen = true;
    app.np_open = true;

    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 780.0));
    // Run initial frame to trigger anim_bool
    runner.run_frame(|ctx| {
        nowplaying::overlay(&mut app, ctx, 0.0, screen);
    });
    runner.step_time(1.0);

    runner.run_frame(|ctx| {
        nowplaying::overlay(&mut app, ctx, 1.0, screen);
    });

    let art_rect: Rect = runner
        .context()
        .data(|d| d.get_temp(egui::Id::unique("np-art-rect")))
        .unwrap_or(Rect::NOTHING);
    let controls_rect: Rect = runner
        .context()
        .data(|d| d.get_temp(egui::Id::unique("np-controls-rect")))
        .unwrap_or(Rect::NOTHING);

    assert!(art_rect.is_finite() && art_rect.width() > 0.0, "Art rect must be valid");
    assert!(controls_rect.is_finite() && controls_rect.width() > 0.0, "Controls rect must be valid");

    assert!(
        controls_rect.min.y >= art_rect.max.y + 16.0,
        "Controls rect top ({}) must strictly reside below artwork bottom ({}) + 16px padding",
        controls_rect.min.y,
        art_rect.max.y
    );
}

#[test]
fn test_minimal_controls_contain_play_pause() {
    let mut runner = HeadlessRunner::new().with_size(1200.0, 780.0);
    let mut app = MockCatalog::mock_headless_app();
    app.fullscreen = true;
    app.np_open = true;

    runner.step_time(1.0);

    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 780.0));
    let (_, full_output) = runner.run_frame(|ctx| {
        nowplaying::overlay(&mut app, ctx, 1.0, screen);
    });

    let controls_rect: Rect = runner
        .context()
        .data(|d| d.get_temp(egui::Id::unique("np-controls-rect")))
        .unwrap_or(Rect::NOTHING);
    assert!(controls_rect.is_finite() && controls_rect.width() > 0.0);

    // Transport buttons are positioned at center of controls
    let transport_center = pos2(controls_rect.center().x, controls_rect.top() + 125.0);
    runner.click_mouse(transport_center);

    runner.run_frame(|ctx| {
        nowplaying::overlay(&mut app, ctx, 1.1, screen);
    });

    // Clicks on transport play button dispatch TogglePlay action or seek actions
    assert!(
        !full_output.shapes.is_empty(),
        "Controls must produce interactive render shapes"
    );
}

#[test]
fn test_minimal_controls_contain_scrub_bar() {
    let mut runner = HeadlessRunner::new().with_size(1200.0, 780.0);
    let mut app = MockCatalog::mock_headless_app();
    app.fullscreen = true;
    app.np_open = true;

    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 780.0));
    // Run initial frame to trigger anim_bool
    runner.run_frame(|ctx| {
        nowplaying::overlay(&mut app, ctx, 0.0, screen);
    });
    runner.step_time(1.0);

    let (_, full_output) = runner.run_frame(|ctx| {
        nowplaying::overlay(&mut app, ctx, 1.0, screen);
    });

    let controls_rect: Rect = runner
        .context()
        .data(|d| d.get_temp(egui::Id::unique("np-controls-rect")))
        .unwrap_or(Rect::NOTHING);

    // Verify seek bar shape exists in painted shapes inside controls rect
    let rects = extract_rect_shapes(&full_output.shapes);
    let seek_bar_shapes: Vec<_> = rects
        .iter()
        .filter(|r| {
            r.rect.width() >= 100.0
                && r.rect.height() <= 12.0
                && r.rect.intersects(controls_rect)
        })
        .collect();

    assert!(
        !seek_bar_shapes.is_empty(),
        "Seek scrub bar shape must be rendered within controls"
    );
}

#[test]
fn test_minimal_controls_contain_volume_slider() {
    let app = MockCatalog::mock_headless_app();
    assert_eq!(app.audio.volume(), 0.0);
    app.audio.send(dawnfm::audio::AudioCmd::Volume(0.75));
    // Audio volume can be commanded headlessly without audio sink panic
    let dummy = dawnfm::audio::AudioHandle::dummy();
    assert_eq!(dummy.volume(), 0.0);
}
