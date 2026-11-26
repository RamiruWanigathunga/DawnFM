//! Tier 2: F22 - Win32 Subclassing & DWM Border Boundaries
//!
//! Validates negative coordinate calculation (multi-monitor), 0x0 window dimensions,
//! minimized window guard (-32000 coordinates), maximized window overscan compensation,
//! and High-DPI coordinate scaling per ORIGINAL_REQUEST §R6.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Win32Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Win32Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

pub fn is_minimized_rect(r: Win32Rect) -> bool {
    r.left <= -30000 || r.top <= -30000
}

pub fn adjust_maximized_overscan(r: Win32Rect, border_padding: i32) -> Win32Rect {
    Win32Rect {
        left: r.left + border_padding,
        top: r.top + border_padding,
        right: r.right - border_padding,
        bottom: r.bottom - border_padding,
    }
}

pub fn scale_border_thickness(base_thickness: i32, scale_factor: f32) -> i32 {
    ((base_thickness as f32) * scale_factor).round() as i32
}

#[derive(Debug, PartialEq)]
pub enum Win32HitTest {
    Client,
    Caption,
    Border,
    Ignore,
}

pub fn hit_test_point(
    window: Win32Rect,
    titlebar_height: i32,
    border_thickness: i32,
    x: i32,
    y: i32,
) -> Win32HitTest {
    if is_minimized_rect(window) || window.width() <= 0 || window.height() <= 0 {
        return Win32HitTest::Ignore;
    }

    // Outer borders
    if x < window.left + border_thickness
        || x >= window.right - border_thickness
        || y < window.top + border_thickness
        || y >= window.bottom - border_thickness
    {
        return Win32HitTest::Border;
    }

    // Titlebar
    if y < window.top + titlebar_height {
        return Win32HitTest::Caption;
    }

    Win32HitTest::Client
}

#[test]
fn test_win32_negative_screen_coords_calc() {
    // Secondary monitor to the left: left = -1920, top = 0, right = 0, bottom = 1080
    let window = Win32Rect {
        left: -1920,
        top: 0,
        right: -920,
        bottom: 700,
    };

    assert_eq!(window.width(), 1000);
    assert_eq!(window.height(), 700);

    // Hit test on negative coordinate titlebar
    let hit = hit_test_point(window, 32, 6, -1500, 16);
    assert_eq!(hit, Win32HitTest::Caption);

    // Hit test on left border
    let hit_border = hit_test_point(window, 32, 6, -1918, 200);
    assert_eq!(hit_border, Win32HitTest::Border);
}

#[test]
fn test_win32_zero_size_window_handling() {
    let zero_window = Win32Rect {
        left: 100,
        top: 100,
        right: 100,
        bottom: 100,
    };
    assert_eq!(zero_window.width(), 0);
    assert_eq!(zero_window.height(), 0);

    let hit = hit_test_point(zero_window, 32, 6, 100, 100);
    assert_eq!(hit, Win32HitTest::Ignore);
}

#[test]
fn test_win32_minimized_window_calc_guard() {
    // Windows sets minimized windows to (-32000, -32000)
    let minimized = Win32Rect {
        left: -32000,
        top: -32000,
        right: -31840,
        bottom: -31970,
    };

    assert!(is_minimized_rect(minimized));
    let hit = hit_test_point(minimized, 32, 6, -32000, -32000);
    assert_eq!(hit, Win32HitTest::Ignore);
}

#[test]
fn test_win32_maximized_overscan_adjustment() {
    // Maximized window on 1920x1080 with Windows +8px invisible frame
    let raw_maximized = Win32Rect {
        left: -8,
        top: -8,
        right: 1928,
        bottom: 1088,
    };

    let adjusted = adjust_maximized_overscan(raw_maximized, 8);
    assert_eq!(adjusted.left, 0);
    assert_eq!(adjusted.top, 0);
    assert_eq!(adjusted.right, 1920);
    assert_eq!(adjusted.bottom, 1080);
    assert_eq!(adjusted.width(), 1920);
    assert_eq!(adjusted.height(), 1080);
}

#[test]
fn test_win32_high_dpi_coordinate_scaling() {
    let base_thickness = 8;

    // 100% DPI (96 DPI)
    assert_eq!(scale_border_thickness(base_thickness, 1.0), 8);
    // 125% DPI (120 DPI)
    assert_eq!(scale_border_thickness(base_thickness, 1.25), 10);
    // 150% DPI (144 DPI)
    assert_eq!(scale_border_thickness(base_thickness, 1.5), 12);
    // 200% DPI (192 DPI)
    assert_eq!(scale_border_thickness(base_thickness, 2.0), 16);
    // 300% DPI (288 DPI)
    assert_eq!(scale_border_thickness(base_thickness, 3.0), 24);
}
