//! Shared headless test harness and simulation infrastructure for Dawn FM E2E tests.

#![allow(dead_code)]

pub mod fixtures;
pub mod mock_backend;
pub mod mock_stream;

pub use fixtures::*;
pub use mock_backend::*;
pub use mock_stream::*;

use egui::{vec2, Context, Event, Key, PointerButton, Pos2, RawInput, Rect, Ui, Vec2};

/// Runs a single headless UI frame closure on a clean `egui::Context`.
///
/// Invariant: Zero GUI window / Metal / Wayland initialization; runs 100% in software.
pub fn run_headless_frame<F: FnOnce(&Context)>(f: F) {
    let ctx = Context::default();
    let mut raw_input = RawInput::default();
    raw_input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 780.0)));
    raw_input.time = Some(0.0);
    let mut opt = Some(f);
    let mut output = ctx.run_pass(raw_input, |ctx| {
        if let Some(f) = opt.take() {
            f(ctx);
        }
    });
    output.textures_delta.clear();
}

/// Alias for `run_headless_frame` matching prompt requirements.
pub fn run_headless_egui_frame<F: FnOnce(&Context)>(f: F) {
    run_headless_frame(f);
}

/// Runs a single headless UI frame closure directly receiving the root `&mut Ui`.
pub fn run_headless_ui<F: FnOnce(&mut Ui)>(f: F) {
    let ctx = Context::default();
    let mut raw_input = RawInput::default();
    raw_input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 780.0)));
    raw_input.time = Some(0.0);
    let mut opt = Some(f);
    let mut output = ctx.run_ui(raw_input, |ui| {
        if let Some(f) = opt.take() {
            f(ui);
        }
    });
    output.textures_delta.clear();
}

/// Headless test driver maintaining simulated virtual time, viewport dimensions, and input queues.
pub struct HeadlessRunner {
    pub ctx: Context,
    pub current_time: f64,
    pub screen_size: Vec2,
    pub mouse_pos: Option<Pos2>,
    pending_events: Vec<Event>,
}

impl HeadlessRunner {
    /// Creates a new headless runner initialized at time 0.0 with default desktop dimensions (1200x780).
    pub fn new() -> Self {
        Self {
            ctx: Context::default(),
            current_time: 0.0,
            screen_size: vec2(1200.0, 780.0),
            mouse_pos: None,
            pending_events: Vec::new(),
        }
    }

    /// Sets custom virtual viewport dimensions (e.g. for ultra-wide, compact, or mobile layouts).
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.screen_size = vec2(width, height);
        self
    }

    /// Advances virtual clock forward by `delta_secs`.
    pub fn step_time(&mut self, delta_secs: f64) {
        self.current_time += delta_secs.max(0.0);
    }

    /// Sets virtual clock to an exact timestamp.
    pub fn set_time(&mut self, time_secs: f64) {
        self.current_time = time_secs;
    }

    /// Enqueues a simulated pointer move event.
    pub fn move_mouse(&mut self, pos: Pos2) {
        self.mouse_pos = Some(pos);
        self.pending_events.push(Event::PointerMoved(pos));
    }

    /// Enqueues a simulated pointer click (press + release) at `pos`.
    pub fn click_mouse(&mut self, pos: Pos2) {
        self.mouse_pos = Some(pos);
        self.pending_events.push(Event::PointerMoved(pos));
        self.pending_events.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        self.pending_events.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        });
    }

    /// Enqueues a simulated keyboard key press event.
    pub fn press_key(&mut self, key: Key) {
        self.pending_events.push(Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        });
        self.pending_events.push(Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        });
    }

    /// Runs a single frame pass with the accumulated simulated state and returns closure result + FullOutput.
    pub fn run_frame<R, F: FnOnce(&Context) -> R>(&mut self, f: F) -> (R, egui::FullOutput) {
        let mut raw_input = RawInput::default();
        raw_input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, self.screen_size));
        raw_input.time = Some(self.current_time);
        raw_input.events = std::mem::take(&mut self.pending_events);

        let mut output_val = None;
        let mut opt = Some(f);
        let mut full_output = self.ctx.run_pass(raw_input, |ctx| {
            if let Some(f) = opt.take() {
                output_val = Some(f(ctx));
            }
        });
        full_output.textures_delta.clear();

        (output_val.expect("closure executed inside run"), full_output)
    }

    /// Runs a single frame pass with `&mut Ui`.
    pub fn run_ui<R, F: FnOnce(&mut Ui) -> R>(&mut self, f: F) -> (R, egui::FullOutput) {
        let mut raw_input = RawInput::default();
        raw_input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, self.screen_size));
        raw_input.time = Some(self.current_time);
        raw_input.events = std::mem::take(&mut self.pending_events);

        let mut output_val = None;
        let mut opt = Some(f);
        let mut full_output = self.ctx.run_ui(raw_input, |ui| {
            if let Some(f) = opt.take() {
                output_val = Some(f(ui));
            }
        });
        full_output.textures_delta.clear();

        (output_val.expect("closure executed inside run"), full_output)
    }

    /// Executes `frames` iterations stepping time by `delta_per_frame` seconds each step.
    pub fn run_frames<F: FnMut(&Context, usize)>(&mut self, frames: usize, delta_per_frame: f64, mut f: F) {
        for i in 0..frames {
            self.step_time(delta_per_frame);
            let mut raw_input = RawInput::default();
            raw_input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, self.screen_size));
            raw_input.time = Some(self.current_time);
            raw_input.events = std::mem::take(&mut self.pending_events);

            let mut full_output = self.ctx.run_pass(raw_input, |ctx| {
                f(ctx, i);
            });
            full_output.textures_delta.clear();
        }
    }

    /// Borrow the underlying egui Context.
    pub fn context(&self) -> &Context {
        &self.ctx
    }
}

impl Default for HeadlessRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Asserts that a numerical floating-point value is within `tolerance` of `expected`.
pub fn assert_within_tolerance(actual: f64, expected: f64, tolerance: f64, message: &str) {
    let diff = (actual - expected).abs();
    assert!(
        diff <= tolerance,
        "{}: actual ({}) differed from expected ({}) by {} > tolerance {}",
        message,
        actual,
        expected,
        diff,
        tolerance
    );
}

/// Asserts that an egui rectangle has non-negative finite bounds.
pub fn assert_bounds_valid(rect: Rect) {
    assert!(rect.is_finite(), "Rect bounds must be finite: {:?}", rect);
    assert!(rect.width() >= 0.0, "Rect width must be >= 0: {:?}", rect);
    assert!(rect.height() >= 0.0, "Rect height must be >= 0: {:?}", rect);
}

/// Asserts that a 2D vector is finite (not NaN or infinite).
pub fn assert_finite_vec2(v: Vec2) {
    assert!(v.x.is_finite() && v.y.is_finite(), "Vec2 must be finite: {:?}", v);
}

/// Asserts that a Color32 has valid rgba channels.
pub fn assert_finite_color(color: egui::Color32) {
    assert!(color.a() > 0 || color.r() == 0, "Color check valid");
}
