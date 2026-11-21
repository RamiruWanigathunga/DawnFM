//! Page bodies. Every page fades and slides in when navigated to.

use crate::sync::MutexExt;
use std::collections::HashSet;
use std::sync::atomic::Ordering;

use egui::{Align2, Color32, Rect, RichText, Sense, Ui, Vec2, pos2, vec2};

use super::widgets::{self, RowOpts, card, chip, h_scroll, named, paint_cover, pill, pill_destructive, shelf_header, track_row};
use super::{Action, App, LibTab, Page, Selection};
#[allow(unused_imports)]
use crate::backend::compact;
use crate::i18n::t;
use crate::model::{Card, ShelfKind, Target, Track};
use crate::theme::{self, icon};

const CARD: f32 = 180.0;

/// How many tiles Apple fits across a feed row at this content width. The site
/// measures from its own viewport, which includes the navigation column this page
/// already sits past.
fn feed_columns(width: f32, square: bool) -> usize {
    let viewport = width + 260.0;
    match square {
        true if viewport < 1260.0 => 5,
        true if viewport < 1580.0 => 6,
        true => 7,
        false if viewport < 1260.0 => 2,
        false if viewport < 1580.0 => 3,
        false => 4,
    }
}

/// One tile of an Apple feed row: a share of the content on wide windows, the site's
/// own narrow width below its breakpoint.
fn feed_tile(ui: &Ui, square: bool) -> f32 {
    let width = ui.available_width();
    let min = if square { 144.0 } else { 270.0 };
    if width < 1024.0 {
        return min;
    }
    let cols = feed_columns(width, square);
    ((width - 20.0 * (cols - 1) as f32) / cols as f32).max(min)
}

pub fn page(app: &mut App, ui: &mut Ui, now: f64) {
    let t = theme::ease_out(((now - app.page_changed) / 0.32) as f32);
    if t < 1.0 {
        // The fade-in finishes even when nothing else is drawing frames.
        super::animate(ui.ctx());
    }
    ui.set_opacity(0.15 + 0.85 * t);
    let page = app.page.clone();
    // Start at the top when navigated to; don't restore previous scroll positions across visits.
    let mut area = egui::ScrollArea::vertical().id_salt(("page", &page, app.page_visit)).auto_shrink(false);
    if std::mem::take(&mut app.scroll_top) || t < 1.0 {
        area = area.vertical_scroll_offset(0.0);
    }
    // The scroll bar hugs the right edge of the pages.
    ui.spacing_mut().scroll.bar_outer_margin = 2.0 + super::titlebar::edge_inset();
    // Dev hook: YTM_SCROLL=<px> pins the page scroll, for screenshots of lower sections.
    if let Some(y) = super::dev_var("YTM_SCROLL").and_then(|v| v.parse::<f32>().ok()) {
        area = area.vertical_scroll_offset(y);
    }
    let out = area.show(ui, |ui| {
        let is_artist = matches!(page, Page::Artist(_));
        if !is_artist {
            // The page's own 28 pt right gap, plus room for the scroll bar beside it.
            ui.set_max_width((ui.available_width() - 48.0).max(0.0));
            ui.add_space(18.0 * (1.0 - t));
        }
        match &page {
            Page::Home => home(app, ui, now),
            Page::New => new(app, ui, now),
            Page::Search(q) => search(app, ui, q, now),
            Page::Album(id) | Page::Playlist(id) => collection(app, ui, id, now),
            Page::Artist(id) => artist(app, ui, id, now),
            Page::Library(tab) => library(app, ui, *tab, now),
            Page::Downloads => downloads(app, ui, now),
            Page::Settings => settings(app, ui),
            Page::Transfer => super::transfer::page(app, ui, now),
            Page::Jam => super::jam::page(app, ui),
        }
        // Room to scroll the last rows clear of the player's frosted strip.
        ui.add_space(if page == Page::Settings { 24.0 } else { 96.0 });
    });
    selection_bar(app, ui.ctx(), &page, out.inner_rect);
    // Infinite scroll on home: fetch the next shelves when nearing the bottom.
    if page == Page::Home {
        let remaining = out.content_size.y - (out.state.clamped_offset().y + out.inner_rect.height());
        if let Some(h) = &mut app.home {
            // A failed fetch reports nothing (it's a quiet request): allow a retry later.
            if h.loading_more.is_some_and(|t| now - t > 20.0) {
                h.loading_more = None;
            }
            if remaining < 700.0 && h.loading_more.is_none()
                && let Some(token) = h.continuation.clone()
            {
                h.loading_more = Some(now);
                app.backend.request(crate::backend::Req::HomeMore(token));
            }
        }
    }
}

/// What every song row needs from the app, gathered once per frame.
pub(super) struct Rows {
    current: Option<String>,
    playing: bool,
    accent: Color32,
    now: f64,
}

impl Rows {
    pub(super) fn new(app: &App, now: f64) -> Self {
        Self {
            current: app.current_id().map(str::to_owned),
            playing: app.audio.status.playing.load(Ordering::Relaxed),
            accent: app.accent_color(),
            now,
        }
    }

    pub(super) fn opts(&self, liked: &HashSet<String>, t: &Track, number: Option<usize>, show_album: bool) -> RowOpts {
        RowOpts {
            number,
            show_album,
            show_plays: false,
            is_album: number.is_some() && !show_album,
            current: self.current.as_deref() == Some(t.id.as_str()),
            playing: self.playing,
            accent: self.accent,
            time: self.now,
            liked: liked.contains(&t.id),
            added_by: None,
            pick: false,
            selected: None,
            remove: None,
            remove_queue: None,
            is_dragged: false,
            draggable: false,
            alt: false,
        }
    }
}

fn check_failed(app: &mut App, ui: &mut Ui) -> bool {
    if app.load_failed {
        ui.add_space(40.0);
        ui.label(RichText::new("Couldn't load this page").color(theme::text_dim()));
        ui.add_space(12.0);
        if pill(ui, "Try again", None, false).clicked() {
            app.refresh();
        }
        true
    } else {
        false
    }
}

/// A page's data hasn't arrived: shimmering placeholders while it loads, a retry if the
/// request failed.
fn placeholder(app: &mut App, ui: &mut Ui, now: f64, rows: usize, card: f32) {
    if check_failed(app, ui) {
        return;
    }
    widgets::skeleton(ui, now, rows, card);
}

/// Album or playlist loading skeleton: cover art on the left, collection metadata and action
/// buttons on the right, followed by track rows.
fn placeholder_collection(app: &mut App, ui: &mut Ui, now: f64) {
    if check_failed(app, ui) {
        return;
    }
    let shimmer = |ui: &Ui, r: Rect, radius: f32| {
        ui.painter().rect_filled(r, radius, theme::glass(0.06));
        widgets::shimmer_band(ui, r, radius, now);
    };
    let side = (ui.available_width() * 0.3).clamp(200.0, 300.0);
    ui.add_space(18.0);
    ui.horizontal_top(|ui| {
        let (img, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
        let radius = theme::card_radius() + 2.0;
        shimmer(ui, img, radius);

        ui.add_space(28.0);
        ui.vertical(|ui| {
            ui.add_space((side - 200.0).max(6.0));
            let title_w = (ui.available_width() * 0.55).clamp(200.0, 380.0);
            let (title_r, _) = ui.allocate_exact_size(vec2(title_w, 32.0), Sense::hover());
            shimmer(ui, title_r, 6.0);

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
                let (avatar_r, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
                shimmer(ui, avatar_r, 17.0);

                let (name_r, _) = ui.allocate_exact_size(vec2(160.0, 18.0), Sense::hover());
                shimmer(ui, name_r, 4.0);
            });

            ui.add_space(12.0);
            let (meta_r, _) = ui.allocate_exact_size(vec2(180.0, 14.0), Sense::hover());
            shimmer(ui, meta_r, 4.0);

            ui.add_space(18.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
                let (play_r, _) = ui.allocate_exact_size(vec2(86.0, 36.0), Sense::hover());
                shimmer(ui, play_r, 18.0);

                let (shuf_r, _) = ui.allocate_exact_size(vec2(98.0, 36.0), Sense::hover());
                shimmer(ui, shuf_r, 18.0);

                let (more_r, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
                shimmer(ui, more_r, 18.0);
            });
        });
    });
    ui.add_space(26.0);
    widgets::skeleton_rows(ui, now, 8);
}

/// Artist page loading skeleton: cinematic full-width hero header with artist name and action
/// buttons, followed by latest release card, top song rows, and album shelf.
fn placeholder_artist(app: &mut App, ui: &mut Ui, now: f64) {
    if check_failed(app, ui) {
        return;
    }
    let shimmer = |ui: &Ui, r: Rect, radius: f32| {
        ui.painter().rect_filled(r, radius, theme::glass(0.06));
        widgets::shimmer_band(ui, r, radius, now);
    };
    let w = ui.available_width();
    let hero_h = hero_height(w);
    let (hero, _) = ui.allocate_exact_size(vec2(w, hero_h), Sense::hover());
    shimmer(ui, hero, 0.0);
    paint_hero_gradient_dissolve(ui.painter(), hero, super::shell::base_color(app));

    let base_x = hero.left() + 28.0;
    let btn_y = hero.bottom() - 36.0;

    let more_rect = Rect::from_center_size(pos2(base_x + 19.0, btn_y), vec2(38.0, 38.0));
    shimmer(ui, more_rect, 19.0);

    let play_center = pos2(base_x + 38.0 + 14.0 + 24.0, btn_y);
    let play_rect = Rect::from_center_size(play_center, vec2(48.0, 48.0));
    shimmer(ui, play_rect, 24.0);

    let name_w = (w * 0.45).clamp(220.0, 380.0);
    let name_r = Rect::from_min_size(pos2(base_x, btn_y - 88.0), vec2(name_w, 44.0));
    shimmer(ui, name_r, 8.0);

    let stats_r = Rect::from_min_size(pos2(base_x, btn_y - 32.0), vec2(160.0, 14.0));
    shimmer(ui, stats_r, 4.0);

    egui::Frame::new().inner_margin(egui::Margin { left: 28, right: 48, top: 0, bottom: 0 }).show(ui, |ui| {
        let cw = ui.available_width();
        ui.add_space(26.0);
        let is_wide = cw >= 780.0;
        if is_wide {
            let left_w = (cw * 0.30).clamp(280.0, 320.0);
            let right_w = (cw - left_w - 32.0).max(360.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(left_w, 0.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    let (head, _) = ui.allocate_exact_size(vec2(140.0, 22.0), Sense::hover());
                    shimmer(ui, head, 4.0);
                    ui.add_space(8.0);
                    let (card_r, _) = ui.allocate_exact_size(vec2(left_w, left_w * 0.65), Sense::hover());
                    shimmer(ui, card_r, 12.0);
                });
                ui.add_space(24.0);
                ui.allocate_ui_with_layout(vec2(right_w, 0.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    let (head, _) = ui.allocate_exact_size(vec2(120.0, 22.0), Sense::hover());
                    shimmer(ui, head, 4.0);
                    ui.add_space(8.0);
                    widgets::skeleton_rows(ui, now, 4);
                });
            });
        } else {
            let (head, _) = ui.allocate_exact_size(vec2(140.0, 22.0), Sense::hover());
            shimmer(ui, head, 4.0);
            ui.add_space(8.0);
            widgets::skeleton_rows(ui, now, 4);
        }

        ui.add_space(26.0);
        let (shelf_head, _) = ui.allocate_exact_size(vec2(160.0, 22.0), Sense::hover());
        shimmer(ui, shelf_head, 4.0);
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            for _ in 0..6 {
                let (r, _) = ui.allocate_exact_size(vec2(CARD, CARD + 50.0), Sense::hover());
                let img_r = Rect::from_min_size(r.min, Vec2::splat(CARD));
                shimmer(ui, img_r, 8.0);
                let t_r = Rect::from_min_size(r.min + vec2(0.0, CARD + 10.0), vec2(CARD * 0.8, 12.0));
                shimmer(ui, t_r, 4.0);
            }
        });
    });
}

/// Search loading skeleton: top result card on the left with top songs on the right,
/// followed by shelves of matching artists and albums.
fn placeholder_search(app: &mut App, ui: &mut Ui, now: f64) {
    if check_failed(app, ui) {
        return;
    }
    let shimmer = |ui: &Ui, r: Rect, radius: f32| {
        ui.painter().rect_filled(r, radius, theme::glass(0.06));
        widgets::shimmer_band(ui, r, radius, now);
    };
    ui.add_space(14.0);
    let is_wide = ui.available_width() >= 780.0;
    if is_wide {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                let (h, _) = ui.allocate_exact_size(vec2(120.0, 24.0), Sense::hover());
                shimmer(ui, h, 4.0);
                ui.add_space(8.0);
                let (card_r, _) = ui.allocate_exact_size(vec2(420.0, 236.0), Sense::hover());
                shimmer(ui, card_r, 14.0);
            });
            ui.add_space(24.0);
            ui.vertical(|ui| {
                let (h, _) = ui.allocate_exact_size(vec2(100.0, 24.0), Sense::hover());
                shimmer(ui, h, 4.0);
                ui.add_space(8.0);
                widgets::skeleton_rows(ui, now, 4);
            });
        });
    } else {
        let (h, _) = ui.allocate_exact_size(vec2(100.0, 24.0), Sense::hover());
        shimmer(ui, h, 4.0);
        ui.add_space(8.0);
        widgets::skeleton_rows(ui, now, 5);
    }
    ui.add_space(26.0);
    let (shelf_h, _) = ui.allocate_exact_size(vec2(140.0, 22.0), Sense::hover());
    shimmer(ui, shelf_h, 4.0);
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        for _ in 0..6 {
            let (r, _) = ui.allocate_exact_size(vec2(CARD, CARD + 50.0), Sense::hover());
            let img_r = Rect::from_min_size(r.min, Vec2::splat(CARD));
            shimmer(ui, img_r, 8.0);
            let t_r = Rect::from_min_size(r.min + vec2(0.0, CARD + 10.0), vec2(CARD * 0.8, 12.0));
            shimmer(ui, t_r, 4.0);
        }
    });
}

/// Explore / New releases page loading skeleton: new releases card shelf, top songs grid rows,
/// and charts shelf.
fn placeholder_new(app: &mut App, ui: &mut Ui, now: f64) {
    if check_failed(app, ui) {
        return;
    }
    let shimmer = |ui: &Ui, r: Rect, radius: f32| {
        ui.painter().rect_filled(r, radius, theme::glass(0.06));
        widgets::shimmer_band(ui, r, radius, now);
    };
    let (h1, _) = ui.allocate_exact_size(vec2(220.0, 24.0), Sense::hover());
    shimmer(ui, h1, 4.0);
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        for _ in 0..6 {
            let (r, _) = ui.allocate_exact_size(vec2(CARD, CARD + 50.0), Sense::hover());
            shimmer(ui, Rect::from_min_size(r.min, Vec2::splat(CARD)), 8.0);
            shimmer(ui, Rect::from_min_size(r.min + vec2(0.0, CARD + 10.0), vec2(CARD * 0.8, 12.0)), 4.0);
        }
    });
    ui.add_space(24.0);
    let (h2, _) = ui.allocate_exact_size(vec2(140.0, 24.0), Sense::hover());
    shimmer(ui, h2, 4.0);
    ui.add_space(12.0);
    widgets::skeleton_rows(ui, now, 4);
    ui.add_space(24.0);
    let (h3, _) = ui.allocate_exact_size(vec2(120.0, 24.0), Sense::hover());
    shimmer(ui, h3, 4.0);
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        for _ in 0..6 {
            let (r, _) = ui.allocate_exact_size(vec2(CARD, CARD + 50.0), Sense::hover());
            shimmer(ui, Rect::from_min_size(r.min, Vec2::splat(CARD)), 8.0);
            shimmer(ui, Rect::from_min_size(r.min + vec2(0.0, CARD + 10.0), vec2(CARD * 0.8, 12.0)), 4.0);
        }
    });
}

/// Track list loading skeleton (History, Liked Songs).
fn placeholder_track_list(app: &mut App, ui: &mut Ui, now: f64, with_actions: bool) {
    if check_failed(app, ui) {
        return;
    }
    let shimmer = |ui: &Ui, r: Rect, radius: f32| {
        ui.painter().rect_filled(r, radius, theme::glass(0.06));
        widgets::shimmer_band(ui, r, radius, now);
    };
    if with_actions {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
            let (p1, _) = ui.allocate_exact_size(vec2(86.0, 36.0), Sense::hover());
            shimmer(ui, p1, 18.0);
            let (p2, _) = ui.allocate_exact_size(vec2(98.0, 36.0), Sense::hover());
            shimmer(ui, p2, 18.0);
            let (p3, _) = ui.allocate_exact_size(vec2(90.0, 36.0), Sense::hover());
            shimmer(ui, p3, 18.0);
        });
        ui.add_space(10.0);
    }
    widgets::skeleton_rows(ui, now, 10);
}

/// Wrapped card grid loading skeleton (Library Playlists, Albums, Artists).
fn placeholder_grid(app: &mut App, ui: &mut Ui, now: f64, card: f32, circle: bool, count: usize, with_new_button: bool) {
    if check_failed(app, ui) {
        return;
    }
    let shimmer = |ui: &Ui, r: Rect, radius: f32| {
        ui.painter().rect_filled(r, radius, theme::glass(0.06));
        widgets::shimmer_band(ui, r, radius, now);
    };
    if with_new_button {
        let (btn_r, _) = ui.allocate_exact_size(vec2(120.0, 34.0), Sense::hover());
        shimmer(ui, btn_r, 17.0);
        ui.add_space(14.0);
    }
    widgets::skeleton_grid(ui, now, card, circle, count);
}

/// What a collection header's round buttons draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CircleGlyph {
    Char(char),
    More,
}

/// The accent colour the app set for this frame.
fn app_accent(ui: &Ui) -> Color32 {
    ui.ctx().data(|d| d.get_temp::<Color32>(egui::Id::unique("accent"))).unwrap_or(theme::RED)
}

/// "ALBUM · 2024" / "PLAYLIST · 42 SONGS · 2 HR 10 MIN": the line under the artist.
pub fn header_meta(c: &crate::backend::Collection) -> String {
    let mut parts = vec![if c.is_album { "ALBUM".to_owned() } else { "PLAYLIST".to_owned() }];
    if c.is_album {
        if let Some(y) = c.subtitle.split(" • ").map(str::trim).find(|p| p.len() == 4 && p.chars().all(|ch| ch.is_ascii_digit())) {
            parts.push(y.to_owned());
        }
    } else {
        let n = c.count.map_or(c.tracks.len() as u64, |n| n);
        parts.push(format!("{n} SONGS"));
    }
    let total: u32 = c.tracks.iter().filter_map(|t| t.duration).sum();
    if total >= 3600 {
        parts.push(format!("{} HR {} MIN", total / 3600, total % 3600 / 60));
    } else if total > 0 {
        parts.push(format!("{} MIN", total / 60));
    }
    parts.join(" · ")
}

pub fn circle_action(ui: &mut Ui, glyph: CircleGlyph, name: &str, active: bool) -> egui::Response {
    // Same 38 px height as the Play pill beside it, so the row shares one centre line.
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::click());
    let resp = named(resp, name).on_hover_text(name).on_hover_cursor(egui::CursorIcon::PointingHand);
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    let h = theme::anim_bool(ui.ctx(), resp.id.with("h"), resp.hovered() || open, 0.15);
    let press = if resp.is_pointer_button_down_on() { 18.3 } else { 19.0 };
    ui.painter().circle_filled(rect.center(), press, theme::glass(0.10 + 0.09 * h));
    let base = if crate::preset::active().app.chip == crate::preset::ChipStyle::Accent { app_accent(ui) } else { theme::text_dim() };
    let color = theme::lerp_color(base, theme::text(), h.max(active as u8 as f32));
    match glyph {
        CircleGlyph::Char(g) => widgets::icon_at(ui, rect.center(), g, 16.0, color),
        CircleGlyph::More => {
            for i in -1..=1 {
                ui.painter().circle_filled(rect.center() + vec2(i as f32 * 5.2, 0.0), 1.8, color);
            }
        }
    }
    resp
}

/// A page's main Play button. Shows Pause while this page's album / playlist / artist
/// is playing, and toggles instead of restarting when it's already loaded.
pub fn play_pill(ui: &mut Ui, acts: &mut Vec<Action>, target: &Target, tracks: &[Track]) -> egui::Response {
    let active = widgets::active_state(ui, target);
    let (label, glyph) = if active == Some(true) { (t!("pause"), icon::PAUSE) } else { (t!("play"), icon::PLAY) };
    let resp = pill(ui, &label, Some(glyph), true);
    if resp.clicked() {
        if active.is_some() {
            acts.push(Action::TogglePlay);
        } else if !tracks.is_empty() {
            acts.push(Action::Play(tracks.to_vec(), 0));
        }
    }
    resp
}

const PEACH: Color32 = Color32::from_rgb(0xE0, 0x7A, 0x5F);
const PEACH_HOVER: Color32 = Color32::from_rgb(0xEB, 0x8B, 0x72);
const PEACH_TEXT: Color32 = Color32::from_rgb(0x1a, 0x14, 0x12);

pub fn peach_pill(ui: &mut Ui, label: &str, glyph: Option<char>) -> egui::Response {
    let font = theme::semibold(14.0);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, PEACH_TEXT);
    let icon_w = if glyph.is_some() { 20.0 } else { 0.0 };
    let w = galley.size().x + icon_w + 32.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 36.0), Sense::click());
    let resp = named(resp, label).on_hover_cursor(egui::CursorIcon::PointingHand);
    let h = if resp.hovered() { 1.0 } else { 0.0 };
    let bg = theme::lerp_color(PEACH, PEACH_HOVER, h);
    let r = 18.0;
    let shrink = if resp.is_pointer_button_down_on() { 1.0 } else { 0.0 };
    ui.painter().rect_filled(rect.shrink(shrink), r, bg);
    let mut x = rect.left() + 16.0;
    let cy = rect.center().y;
    if let Some(g) = glyph {
        widgets::icon_at(ui, pos2(x + 4.0, cy), g, 15.0, PEACH_TEXT);
        x += icon_w;
    }
    ui.painter().galley(pos2(x, cy - galley.size().y / 2.0), galley, PEACH_TEXT);
    resp
}

pub fn peach_circle(ui: &mut Ui, name: &str, glyph: CircleGlyph) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());
    let resp = named(resp, name).on_hover_text(name).on_hover_cursor(egui::CursorIcon::PointingHand);
    let h = if resp.hovered() { 1.0 } else { 0.0 };
    let bg = theme::lerp_color(PEACH, PEACH_HOVER, h);
    let shrink = if resp.is_pointer_button_down_on() { 1.0 } else { 0.0 };
    ui.painter().circle_filled(rect.center(), 18.0 - shrink, bg);
    match glyph {
        CircleGlyph::Char(g) => widgets::icon_at(ui, rect.center(), g, 16.0, PEACH_TEXT),
        CircleGlyph::More => {
            widgets::paint_more_dots(ui.painter(), rect.center(), 5.0, 1.8, PEACH_TEXT);
        }
    }
    resp
}

pub fn metadata_badge(ui: &mut Ui, glyph: Option<char>, label: &str) {
    let font = theme::semibold(11.5);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, theme::text_dim());
    let icon_w = if glyph.is_some() { 16.0 } else { 0.0 };
    let badge_w = galley.size().x + icon_w + 18.0;
    let (rect, _) = ui.allocate_exact_size(vec2(badge_w, 24.0), Sense::hover());
    let r = 12.0;
    ui.painter().rect_filled(rect, r, theme::glass(0.12));
    ui.painter().rect_stroke(rect, r, egui::Stroke::new(1.0, theme::glass(0.16)), egui::StrokeKind::Inside);
    let mut x = rect.left() + 9.0;
    let cy = rect.center().y;
    if let Some(g) = glyph {
        ui.painter().text(pos2(x, cy), Align2::LEFT_CENTER, g.to_string(), theme::regular(12.0), theme::text_dim());
        x += icon_w;
    }
    ui.painter().galley(pos2(x, cy - galley.size().y / 2.0), galley, theme::text_dim());
}

pub fn collection_genre(c: &crate::backend::Collection) -> &'static str {
    let text = format!("{} {} {}", c.title, c.subtitle, c.description.as_deref().unwrap_or_default()).to_lowercase();
    if text.contains("electro") || text.contains("synth") || text.contains("dance") || text.contains("house") || text.contains("techno") || text.contains("chromeo") {
        "ELECTRONIC"
    } else if text.contains("hip hop") || text.contains("rap") || text.contains("trap") {
        "HIP-HOP"
    } else if text.contains("r&b") || text.contains("soul") || text.contains("funk") {
        "R&B"
    } else if text.contains("rock") || text.contains("metal") || text.contains("punk") {
        "ROCK"
    } else if text.contains("jazz") {
        "JAZZ"
    } else if text.contains("classical") {
        "CLASSICAL"
    } else if text.contains("pop") {
        "POP"
    } else if c.is_album {
        "ELECTRONIC"
    } else {
        "PLAYLIST"
    }
}

pub fn paint_ambient_glow(painter: &egui::Painter, rect: Rect, color: Color32) {
    let center = pos2(rect.left() + rect.width() * 0.35, rect.top() + rect.height() * 0.32);
    let radius = rect.width().max(rect.height()) * 0.65;
    let dark_bg = Color32::from_rgb(0x1E, 0x1F, 0x22);
    let mut mesh = egui::Mesh::default();
    let c_center = theme::with_alpha(color, 0.45);
    let c_edge = Color32::TRANSPARENT;
    mesh.colored_vertex(center, c_center);
    const SEGMENTS: usize = 24;
    for i in 0..=SEGMENTS {
        let angle = (i as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
        let p = center + vec2(angle.cos() * radius, angle.sin() * radius * 0.7);
        mesh.colored_vertex(p, c_edge);
        if i > 0 {
            mesh.add_triangle(0, i as u32, i as u32 + 1);
        }
    }
    painter.add(egui::Shape::Mesh(mesh.into()));
    super::shell::vgradient(painter, rect, Color32::TRANSPARENT, theme::with_alpha(dark_bg, 0.94));
}

pub fn paint_hero_gradient_dissolve(painter: &egui::Painter, rect: Rect, bg: Color32) {
    const STOPS: [(f32, u8); 6] = [
        (0.00, 0),
        (0.35, 0),
        (0.55, 60),
        (0.72, 140),
        (0.88, 220),
        (1.00, 255),
    ];
    for w in STOPS.windows(2) {
        let (y0_pct, a0) = w[0];
        let (y1_pct, a1) = w[1];
        let seg = Rect::from_min_max(
            pos2(rect.left(), rect.top() + rect.height() * y0_pct),
            pos2(rect.right(), rect.top() + rect.height() * y1_pct),
        );
        let c0 = Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), a0);
        let c1 = Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), a1);
        super::shell::vgradient(painter, seg, c0, c1);
    }
}

pub fn hero_height(viewport_width: f32) -> f32 {
    (viewport_width * 0.40).clamp(260.0, 480.0)
}

pub fn filter_collection_tracks<'a>(tracks: &'a [Track], needle: &str) -> Vec<&'a Track> {
    let q = needle.trim().to_lowercase();
    if q.is_empty() {
        return tracks.iter().collect();
    }
    tracks
        .iter()
        .filter(|t| {
            t.title.to_lowercase().contains(&q)
                || t.artist_line().to_lowercase().contains(&q)
                || t.album.as_ref().is_some_and(|a| a.name.to_lowercase().contains(&q))
        })
        .collect()
}

fn card_row(ui: &mut Ui, id: &str, title: &str, cards: &[Card], acts: &mut Vec<Action>) {
    if cards.is_empty() {
        return;
    }
    let (l, r) = shelf_header(ui, id, title, None, None, true);
    let nudge = r as i32 - l as i32;
    let tile = feed_tile(ui, true);
    h_scroll(ui, id, nudge, cards.len(), tile, 20.0, |ui| {
        ui.spacing_mut().item_spacing.x = 20.0;
        for c in cards {
            card(ui, c, tile, false, acts);
        }
    });
}

/// Songs laid out in columns of `rows`, scrolling sideways (Quick picks). Clicking a
/// song starts its radio. Returns whether the arrows reached the end.
#[allow(clippy::too_many_arguments)]
fn song_grid(ui: &mut Ui, rows_ctx: &Rows, liked: &HashSet<String>, acts: &mut Vec<Action>, id: &str, title: &str, strap: Option<&str>, tracks: &[Track], rows: usize) -> bool {
    let (l, r) = shelf_header(ui, id, title, strap, None, true);
    let nudge = r as i32 - l as i32;
    let col_w = feed_tile(ui, false);
    let mut picked = None;
    let (_, reached_end) = h_scroll(ui, id, nudge, tracks.len().div_ceil(rows.max(1)), col_w, 20.0, |ui| {
        ui.spacing_mut().item_spacing.x = 20.0;
        for (c, col) in tracks.chunks(rows.max(1)).enumerate() {
            ui.allocate_ui(vec2(col_w, rows as f32 * 60.0), |ui| {
                ui.set_width(col_w);
                ui.vertical(|ui| {
                    for (i, t) in col.iter().enumerate() {
                        let o = RowOpts {
                            alt: i % 2 == 0,
                            ..rows_ctx.opts(liked, t, None, false)
                        };
                        if track_row(ui, t, &o, acts).clicked() {
                            picked = Some(c * rows + i);
                        }
                    }
                });
            });
        }
    });
    if let Some(i) = picked {
        acts.push(Action::Play(tracks.to_vec(), i));
    }
    reached_end
}

/// Where a song list lives: the page its selection belongs to, and whether it's your own
/// playlist (so songs can be taken out of it).
struct ListCtx<'a> {
    sel: &'a mut Selection,
    page: &'a Page,
    own: bool,
    /// Show play counts even though these aren't album songs (the artist's Top Songs).
    plays: bool,
}

/// A long song list; only the rows on screen are built. Clicking one plays `tracks` from it,
/// or picks it while selecting: Ctrl-click picks one song, Shift-click a run of them.
#[allow(clippy::too_many_arguments)]
fn list(ui: &mut Ui, rows: &Rows, liked: &HashSet<String>, tracks: &[Track], show_album: bool, number: impl Fn(usize, &Track) -> Option<usize>, cx: ListCtx, acts: &mut Vec<Action>) {
    let ListCtx { sel, page, own, plays } = cx;
    let selecting = sel.on(page);
    let mods = ui.input(|i| i.modifiers);
    widgets::visible_rows(ui, tracks.len(), |ui, i| {
        let t = &tracks[i];
        let num = number(i, t);
        let alt = num.map_or(i % 2 == 0, |n| n % 2 == 1);
        let o = RowOpts {
            show_plays: plays,
            selected: selecting.then(|| sel.rows.contains(&i)),
            remove: own.then_some(i),
            alt,
            ..rows.opts(liked, t, num, show_album)
        };
        if track_row(ui, t, &o, acts).clicked() {
            if selecting || mods.command || mods.shift {
                sel.pick(page, i, mods.shift);
            } else {
                acts.push(Action::Play(tracks.to_vec(), i));
            }
        }
    });
}

/// A list page's "Select" button: turns select mode on and off.
fn select_pill(ui: &mut Ui, sel: &mut Selection, page: &Page) {
    let on = sel.on(page);
    if pill(ui, if on { "Done" } else { "Select" }, Some(icon::LIST_CHECKS), false).clicked() {
        if on { sel.clear() } else { sel.start(page) }
    }
}

/// The songs a page lists, and whether they're your own playlist's.
fn selectable<'a>(app: &'a App, page: &Page) -> Option<(&'a [Track], bool)> {
    match page {
        Page::Album(id) | Page::Playlist(id) => {
            app.opened(id).map(|o| &o.c).map(|c| (c.tracks.as_slice(), app.logged_in && c.owned && !c.is_album))
        }
        Page::Library(LibTab::Songs) => app.opened(super::LIKED_MUSIC).map(|o| (o.c.tracks.as_slice(), false)),
        Page::Library(LibTab::History) => app.history.as_deref().map(|h| (h, false)),
        _ => None,
    }
}

/// While selecting: a bar floating over the bottom of the page with what to do with the picks.
fn selection_bar(app: &mut App, ctx: &egui::Context, page: &Page, area: Rect) {
    if !app.selection.on(page) {
        return;
    }
    let Some((tracks, own)) = selectable(app, page) else { return };
    let n = tracks.len();
    let picked: Vec<Track> = app.selection.rows.iter().filter_map(|&i| tracks.get(i).cloned()).collect();
    let all = !picked.is_empty() && picked.len() == n;
    let all_liked = !picked.is_empty() && picked.iter().all(|t| app.liked.contains(&t.id));
    // Labelled buttons when there's room, icons with tooltips when the window is narrow.
    let wide = area.width() > 820.0;
    let button = |ui: &mut Ui, label: &str, glyph: char, primary: bool| {
        if wide { pill(ui, label, Some(glyph), primary) } else { widgets::icon_button(ui, glyph, 18.0, label, false) }
    };
    let mut acts = Vec::new();
    let mut toggle_all = false;
    let mut done = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    egui::Area::new(egui::Id::unique("selection-bar"))
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(pos2(area.center().x, area.bottom() - 18.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(theme::menu_fill())
                .corner_radius(14)
                .inner_margin(egui::Margin { left: 20, right: 10, top: 8, bottom: 8 })
                .shadow(egui::epaint::Shadow { offset: [0, 8], blur: 24, spread: 0, color: theme::shade(0.45) })
                .show(ui, |ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let count = if picked.is_empty() { "Select songs".to_owned() } else { format!("{} selected", picked.len()) };
                        ui.label(RichText::new(count).font(theme::semibold(14.0)).color(theme::text()));
                        ui.add_space(10.0);
                        if !picked.is_empty() {
                            if button(ui, "Add to playlist", icon::LIST_PLUS, true).clicked() {
                                acts.push(Action::AddToPlaylist(picked.clone()));
                            }
                            if own && button(ui, "Remove from playlist", icon::LIST_MINUS, false).clicked() {
                                acts.push(Action::RemoveFromPlaylist(app.selection.rows.iter().copied().collect()));
                            }
                            if button(ui, if all_liked { "Remove like" } else { "Like" }, icon::HEART, false).clicked() {
                                acts.push(Action::LikeMany(picked.clone(), !all_liked));
                            }
                            if button(ui, "Add to queue", icon::LIST_MUSIC, false).clicked() {
                                acts.extend(picked.iter().cloned().map(Action::Enqueue));
                            }
                        }
                        toggle_all = button(ui, if all { "Select none" } else { "Select all" }, icon::LIST_CHECKS, false).clicked();
                        done |= widgets::icon_button(ui, icon::X, 18.0, "Done selecting", false).clicked();
                    });
                });
        });
    if toggle_all {
        let page = page.clone();
        app.selection.start(&page);
        if !all {
            app.selection.rows = (0..n).collect();
        }
    }
    if done || !acts.is_empty() {
        app.selection.clear();
    }
    app.actions.extend(acts);
}

// ------------------------------------------------------------------ home

/// A subtitle without its "1.2M plays" / "30K views" part.
fn without_counts(s: &str) -> String {
    let count = |p: &str| {
        let p = p.trim().to_ascii_lowercase();
        ["plays", "play", "views", "view"].iter().any(|w| p.ends_with(&format!(" {w}")) && p.starts_with(|c: char| c.is_ascii_digit()))
    };
    s.split(" • ").filter(|p| !count(p)).collect::<Vec<_>>().join(" • ")
}

fn home_greeting() -> &'static str {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    let time_t = now as libc::time_t;
    let hour = unsafe {
        if !libc::localtime_r(&time_t, tm.as_mut_ptr()).is_null() {
            tm.assume_init().tm_hour
        } else {
            12
        }
    };
    match hour {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    }
}

fn home(app: &mut App, ui: &mut Ui, now: f64) {
    let active_mood = app.home.as_ref().and_then(|h| h.chips.iter().find(|c| c.selected).map(|c| c.title.clone()));
    let greeting;
    let title = if let Some(m) = &active_mood {
        m.as_str()
    } else {
        greeting = home_greeting();
        greeting
    };
    page_title(ui, title);
    if let Some(h) = &mut app.home
        && !h.chips.is_empty()
    {
        ui.add_space(8.0);
        let mut clicked_chip = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for c in &h.chips {
                if chip(ui, &c.title, c.selected).clicked() {
                    let new_selected = !c.selected;
                    let param = if new_selected { c.params.clone() } else { None };
                    clicked_chip = Some((c.title.clone(), param, new_selected));
                }
            }
        });
        if let Some((title, param, new_selected)) = clicked_chip {
            for c in &mut h.chips {
                c.selected = if new_selected { c.title == title } else { false };
            }
            app.actions.push(Action::HomeChip(param));
        }
        ui.add_space(8.0);
    }
    // A refresh keeps showing the current feed; the skeleton is only for an empty one.
    if app.home.as_ref().is_none_or(|h| app.home_loading && h.shelves.is_empty()) {
        placeholder(app, ui, now, 3, CARD);
        return;
    }
    let rows = Rows::new(app, now);
    let Some(h) = &app.home else { return };
    if h.shelves.is_empty() {
        ui.add_space(40.0);
        ui.label(RichText::new("Nothing here yet").color(theme::text_dim()));
    }
    let is_mood = active_mood.is_some();
    let mut more_picks = None;
    for (i, s) in h.shelves.iter().enumerate() {
        if is_mood {
            let lower = s.title.to_lowercase();
            if lower.contains("listen again")
                || lower.contains("quick picks")
                || lower.contains("forgotten favorites")
                || lower.contains("mixed for you")
                || lower.contains("podcast")
                || lower.contains("episode")
            {
                continue;
            }
        }
        let id = format!("home-{i}-{}-{}", s.title, app.page_visit);
        match &s.kind {
            ShelfKind::Cards(cards) => {
                let (strap, thumb) = if s.title == "Listen again" { (None, None) } else { (s.strapline.as_deref(), s.strap_thumb.as_deref()) };
                let (l, r) = shelf_header(ui, &id, &s.title, strap, thumb, true);
                let nudge = r as i32 - l as i32;
                let tile = if i == 0 { (feed_tile(ui, true) * 1.12).min(240.0) } else { feed_tile(ui, true) };
                let acts = &mut app.actions;
                let liked = &app.liked;
                h_scroll(ui, &id, nudge, cards.len(), tile, 20.0, |ui| {
                    ui.spacing_mut().item_spacing.x = 20.0;
                    for c in cards {
                        let is_liked = matches!(&c.target, Target::Song(t) if liked.contains(&t.id));
                        // Home shows no play or view counts.
                        let c = Card { subtitle: without_counts(&c.subtitle), ..c.clone() };
                        card(ui, &c, tile, is_liked, acts);
                    }
                });
            }
            ShelfKind::Songs { tracks, rows: n } => {
                let shown: Vec<Track> = tracks.iter().map(|t| Track { plays: None, ..t.clone() }).collect();
                let reached_end = song_grid(ui, &rows, &app.liked, &mut app.actions, &id, &s.title, s.strapline.as_deref(), &shown, *n);
                if !is_mood && s.title == "Quick picks" && reached_end && !h.picks_loading && !h.picks_expanded {
                    more_picks = tracks.last().map(|t| t.id.clone());
                }
            }
        }
    }
    let loading_more = h.loading_more.is_some();
    if let Some(seed) = more_picks
        && let Some(h) = &mut app.home
    {
        h.picks_loading = true;
        app.backend.request(crate::backend::Req::MoreQuickPicks(seed));
    }
    if loading_more {
        widgets::skeleton(ui, now, 1, CARD);
    }
}

// ------------------------------------------------------------------ new

fn new(app: &mut App, ui: &mut Ui, now: f64) {
    page_title(ui, "New");
    if app.explore.is_none() {
        placeholder_new(app, ui, now);
        return;
    }
    let rows = Rows::new(app, now);
    let Some(e) = &app.explore else { return };
    let id_new = format!("ex-new-{}", app.page_visit);
    card_row(ui, &id_new, if app.logged_in { "New releases for you" } else { "Popular new releases" }, &e.new_albums, &mut app.actions);
    if !e.top.is_empty() {
        let id_top = format!("ex-top-{}", app.page_visit);
        song_grid(ui, &rows, &app.liked, &mut app.actions, &id_top, "Top songs", None, &e.top, 4);
    }
    let id_charts = format!("ex-charts-{}", app.page_visit);
    card_row(ui, &id_charts, "Charts", &e.charts, &mut app.actions);
}

pub(super) fn page_title(ui: &mut Ui, title: &str) {
    // Left-aligned with the page's content, a little below the top edge.
    ui.add_space(18.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::hover());
    ui.painter().text(pos2(rect.left(), rect.center().y), Align2::LEFT_CENTER, title, theme::bold(34.0), theme::text());
}

// ------------------------------------------------------------------ search

fn search(app: &mut App, ui: &mut Ui, q: &str, now: f64) {
    let pick = super::transfer::pick_banner(app, ui);
    page_title(ui, "Search");
    ui.add_space(10.0);
    // Coming from a suggestion, the sidebar or a link, the field says what's shown.
    // Only when the route changed: clearing the box leaves the route alone until Enter.
    if app.search_synced != q {
        app.search_synced = q.to_owned();
        app.search_text = q.trim().to_owned();
    }
    super::shell::search_field(app, ui, ui.available_width().min(560.0));
    if q.trim().is_empty() {
        ui.add_space(26.0);
        recent_searches(ui, app);
        return;
    }
    if app.search.as_ref().is_none_or(|r| r.query != q) {
        ui.add_space(14.0);
        placeholder_search(app, ui, now);
        return;
    }
    let rows = Rows::new(app, now);
    let Some(r) = &app.search else { return };
    let acts = &mut app.actions;
    let liked = &app.liked;
    if r.tracks.is_empty() && r.artists.is_empty() && r.albums.is_empty() && r.playlists.is_empty() {
        ui.add_space(18.0);
        ui.label(RichText::new("No results").font(theme::bold(20.0)).color(theme::text()));
        return;
    }
    ui.add_space(14.0);
    // Top result: the first artist when the query names one, otherwise the first song.
    ui.horizontal_top(|ui| {
        if let Some(top) = r.artists.first().filter(|a| a.title.eq_ignore_ascii_case(q.trim())) {
            top_result(ui, top, false, acts);
        } else if let Some(t) = r.tracks.first() {
            let c = Card {
                title: t.title.clone(),
                subtitle: format!("Song • {}", t.artist_line()),
                thumbs: t.thumbs.clone(),
                target: Target::Song(t.clone()),
                round: false,
                links: t.artists.clone(),
            };
            top_result(ui, &c, pick, acts);
        }
        ui.add_space(24.0);
        ui.vertical(|ui| {
            ui.label(RichText::new("Songs").font(theme::bold(24.0)));
            ui.add_space(8.0);
            for (i, t) in r.tracks.iter().take(4).enumerate() {
                let o = RowOpts { pick, alt: i % 2 == 0, ..rows.opts(liked, t, None, false) };
                if track_row(ui, t, &o, acts).clicked() {
                    acts.push(if pick { Action::TransferPick(t.clone()) } else { Action::Play(r.tracks.clone(), i) });
                }
            }
        });
    });
    card_row(ui, "s-artists", "Artists", &r.artists, acts);
    card_row(ui, "s-albums", "Albums", &r.albums, acts);
    if r.tracks.len() > 4 {
        ui.add_space(26.0);
        ui.label(RichText::new("More songs").font(theme::bold(24.0)));
        ui.add_space(10.0);
        for (i, t) in r.tracks.iter().enumerate().skip(4) {
            let o = RowOpts { pick, alt: i % 2 == 0, ..rows.opts(liked, t, None, true) };
            if track_row(ui, t, &o, acts).clicked() {
                acts.push(if pick { Action::TransferPick(t.clone()) } else { Action::Play(r.tracks.clone(), i) });
            }
        }
    }
    card_row(ui, "s-pl", "Playlists", &r.playlists, acts);
}

/// Apple's search landing: what was opened from the last few searches, with its artwork.
fn recent_searches(ui: &mut Ui, app: &mut App) {
    if app.recent_searches.is_empty() {
        ui.label(RichText::new("Search for artists, songs, albums and playlists.").font(theme::regular(15.0)).color(theme::text_dim()));
        return;
    }
    ui.label(RichText::new("Recently Searched").font(theme::bold(22.0)).color(theme::text()));
    ui.add_space(12.0);
    let w = ui.available_width().min(620.0);
    // The rows are cloned so a removal can be applied after the list is drawn.
    let recents = app.recent_searches.clone();
    let mut remove = None;
    for (i, r) in recents.iter().enumerate() {
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 64.0), Sense::click());
        let resp = named(resp, &r.title).on_hover_cursor(egui::CursorIcon::PointingHand);
        let h = if resp.contains_pointer() { 1.0 } else { 0.0 };
        ui.painter().rect_filled(rect, 10.0, theme::glass(0.05 + 0.06 * h));
        let art = Rect::from_min_size(rect.left_top() + vec2(8.0, 8.0), Vec2::splat(48.0));
        let radius = if r.round { 24.0 } else { 6.0 };
        match r.thumb.as_deref().filter(|u| !u.is_empty()) {
            Some(url) => {
                let thumbs = [crate::model::Thumb { url: url.to_owned(), width: 300, height: 300 }];
                paint_cover(ui, art, &thumbs, radius, 1.0);
            }
            None => {
                ui.painter().rect_filled(art, radius, theme::glass(0.1));
            }
        }
        let x = art.right() + 14.0;
        widgets::text_at(ui, pos2(x, rect.center().y - 17.0), &r.title, theme::semibold(15.0), theme::text(), rect.right() - x - 12.0);
        widgets::text_at(ui, pos2(x, rect.center().y + 4.0), &r.subtitle, theme::regular(13.0), theme::text_dim(), rect.right() - x - 12.0);
        egui::Popup::context_menu(&resp).show(|ui| {
            ui.set_min_width(190.0);
            if widgets::menu_action(ui, None, "Remove from Recently Searched") {
                remove = Some(i);
            }
        });
        if resp.clicked() {
            app.actions.push(match &r.target {
                Some(t) => Action::Open(t.clone()),
                // An entry from an earlier version kept only the search.
                None => Action::Go(Page::Search(r.query.clone())),
            });
        }
        ui.add_space(6.0);
    }
    if let Some(i) = remove {
        app.recent_searches.remove(i);
    }
}

/// `pick`: choosing a transfer match, so clicking a song picks it.
fn top_result(ui: &mut Ui, c: &Card, pick: bool, acts: &mut Vec<Action>) {    ui.vertical(|ui| {
        ui.label(RichText::new("Top result").font(theme::bold(24.0)));
        ui.add_space(8.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(420.0, 236.0), Sense::click());
        let resp = named(resp, &c.title).on_hover_cursor(egui::CursorIcon::PointingHand);
        let h = if resp.contains_pointer() { 1.0 } else { 0.0 };
        ui.painter().rect_filled(rect, 14.0, theme::glass(0.07 + 0.05 * h));
        let img = Rect::from_min_size(rect.min + vec2(20.0, 20.0), Vec2::splat(110.0));
        paint_cover(ui, img, &c.thumbs, if c.round { 55.0 } else { theme::card_radius() }, 1.0);
        let x = rect.left() + 20.0;
        widgets::text_at(ui, pos2(x, img.bottom() + 16.0), &c.title, theme::bold(26.0), theme::text(), rect.width() - 40.0);
        widgets::linked_text(ui, pos2(x, img.bottom() + 52.0), &c.subtitle, &c.links, theme::regular(14.0), theme::text_dim(), rect.width() - 40.0, acts);
        // Floating play button
        let pc = rect.right_bottom() + vec2(-44.0, -44.0 + 6.0 * (1.0 - h));
        let pr = Rect::from_center_size(pc, Vec2::splat(52.0));
        let active = widgets::active_state(ui, &c.target);
        let playing = active == Some(true);
        let label = format!("{} {}", if playing { "Pause" } else { "Play" }, c.title);
        let p = named(ui.interact(pr, resp.id.with("play"), Sense::click()), &label).on_hover_cursor(egui::CursorIcon::PointingHand);
        ui.painter().circle_filled(pc, 26.0, theme::with_alpha(Color32::WHITE, 0.4 + 0.6 * h));
        let nudge = if playing { 0.0 } else { 1.5 };
        widgets::play_pause_glyph(ui, pc + vec2(nudge, 0.0), 10.0, Color32::from_gray(15), playing);
        if p.clicked() {
            acts.push(match &c.target {
                _ if active.is_some() => Action::TogglePlay,
                Target::Song(t) => Action::Radio(t.clone()),
                t => Action::PlayCollection(t.clone(), false),
            });
        } else if resp.clicked() {
            acts.push(match &c.target {
                Target::Song(t) if pick => Action::TransferPick(t.clone()),
                t => Action::Open(t.clone()),
            });
        }
    });
}

// ------------------------------------------------------------------ album / playlist

pub fn collection(app: &mut App, ui: &mut Ui, id: &str, now: f64) {
    if app.opened(id).is_none() {
        placeholder_collection(app, ui, now);
        return;
    }
    let rows = Rows::new(app, now);
    // The header's motion sleeve, asked for before the release is borrowed below.
    let (release, artist) = match app.opened(id) {
        Some(o) => (o.c.title.clone(), o.c.artists.first().map(|a| a.name.clone()).unwrap_or_else(|| o.c.byline())),
        None => (String::new(), String::new()),
    };
    app.apple_cover(&release, &artist, None);
    let artist_id = app.opened(id).and_then(|o| o.c.artists.first()).and_then(|a| a.id.clone());
    if let Some(aid) = &artist_id
        && !app.artist_photos.contains_key(aid)
    {
        app.artist_photos.insert(aid.clone(), Vec::new());
        app.backend.request(crate::backend::Req::ArtistPhoto(aid.clone()));
    }
    // The cover: a little under a third of the page, as in Music.
    let side = (ui.available_width() * 0.3).clamp(200.0, 300.0);
    let px = (side * ui.ctx().pixels_per_point()).round().max(256.0) as u32;
    let sprite = app.apple_sprite(ui.ctx(), &release, &artist, px);
    let Some(o) = app.lists.get(id) else { return };
    let (c, more) = (&o.c, app.receiving(id));
    let acts = &mut app.actions;
    let tracks = &c.tracks;
    let is_album = c.is_album;
    // The artist page links here: its rows show the songs' play counts.
    let top_songs = app.artist.as_ref().is_some_and(|a| a.top_playlist.as_deref() == Some(id));
    let filter_on = app.coll_search.as_ref().is_some_and(|(fid, _)| fid == id);
    let selecting = app.selection.on(&app.page);
    let mut toggle_search = false;
    let mut start_select = false;
    let mut stop_select = false;
    let photo = artist_id.as_ref().and_then(|a| app.artist_photos.get(a)).filter(|t| !t.is_empty());
    ui.add_space(18.0);
    ui.horizontal_top(|ui| {
        let (img, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
        let radius = theme::card_radius() + 2.0;
        // Light mode needs a far softer shadow: the dark one reads as a grey halo.
        let (blur, alpha) = if theme::light() { (26.0, 0.14) } else { (40.0, 0.5) };
        widgets::soft_shadow(ui, img, radius, blur, alpha);
        paint_cover(ui, img, &c.thumbs, radius, 1.0);
        if let Some(s) = &sprite {
            widgets::paint_motion(ui, img, radius, s);
        }
        ui.painter().rect_stroke(img, radius, egui::Stroke::new(1.0, theme::glass(0.08)), egui::StrokeKind::Inside);
        ui.add_space(28.0);
        ui.vertical(|ui| {
            let target = if is_album { Target::Album(id.to_owned()) } else { Target::Playlist(id.to_owned()) };
            // Liked Music and Episodes for later are the account's own lists: they can't be
            // saved, removed, edited or deleted.
            let auto = matches!(id, "LM" | "SE");
            let saved = app.library.as_ref().is_some_and(|l| {
                let cards = if is_album { &l.albums } else { &l.playlists };
                cards.iter().any(|card| card.target == target)
            });
            // Saving only applies to someone else's playlist or an album.
            let save_id = if is_album { c.save_id.clone() } else { (!c.owned).then(|| id.to_owned()) };
            let owned = !is_album && c.owned;
            // Bottom-aligned with the cover, as in Music: title, artist, kind and year, buttons.
            // The account's own lists have no byline row (it'd only name the listener), so
            // the rest sits lower by its height to stay bottom-aligned.
            ui.add_space((side - 200.0).max(6.0) + if auto { 42.0 } else { 0.0 });
            let title = widgets::galley_wrapped(ui, &c.title, theme::bold(34.0), theme::text(), ui.available_width(), 2);
            ui.label(title);
            if !auto {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
                    if let Some(a) = c.artists.first() {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 17.0, theme::glass(0.10));
                        match photo {
                            Some(t) => paint_cover(ui, r, t, 17.0, 1.0),
                            None => widgets::icon_at(ui, r.center(), icon::CIRCLE_USER, 18.0, theme::text_dim()),
                        }
                        let name = widgets::galley(ui, &a.name, theme::semibold(20.0), theme::text(), 360.0);
                        let (nr, resp) = ui.allocate_exact_size(name.size(), Sense::click());
                        let resp = named(resp, &a.name).on_hover_cursor(egui::CursorIcon::PointingHand);
                        let fg = if resp.hovered() { theme::lerp_color(theme::text(), app_accent(ui), 0.5) } else { theme::text() };
                        ui.painter().galley(nr.min, name, fg);
                        if resp.clicked()
                            && let Some(aid) = &a.id
                        {
                            acts.push(Action::Open(Target::Artist(aid.clone())));
                        }
                    } else {
                        ui.label(RichText::new(c.byline()).font(theme::semibold(18.0)).color(theme::text_dim()));
                    }
                    // Saved to the library: a tick; otherwise a plus to save it.
                    if let Some(save_id) = save_id.clone() {
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                        let name = if saved { "Remove from library" } else { "Save to library" };
                        let resp = named(resp, name).on_hover_text(name).on_hover_cursor(egui::CursorIcon::PointingHand);
                        let h = if resp.hovered() { 1.0 } else { 0.0 };
                        ui.painter().rect_filled(r, 8.0, theme::glass(0.10 + 0.08 * h));
                        widgets::icon_at(ui, r.center(), if saved { icon::CHECK } else { icon::LIST_PLUS }, 17.0, theme::text());
                        if resp.clicked() {
                            acts.push(Action::SaveCollection(save_id, !saved));
                        }
                    }
                });
            }
            ui.add_space(8.0);
            ui.label(RichText::new(header_meta(c)).font(theme::semibold(13.0)).color(theme::text_dim()));
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
                play_pill(ui, acts, &target, tracks);
                if pill(ui, &t!("shuffle"), Some(icon::SHUFFLE), true).clicked() && !tracks.is_empty() {
                    let mut t = tracks.clone();
                    super::shuffle(&mut t);
                    acts.push(Action::Play(t, 0));
                }
                // Round actions at the right end of the row.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 0.0);
                    let more = circle_action(ui, CircleGlyph::More, "More", false);
                    egui::Popup::menu(&more).show(|ui| {
                        ui.set_width(200.0);
                        ui.add_space(2.0);
                        let mut any = false;
                        if app.logged_in && !auto && !tracks.is_empty()
                            && widgets::menu_action(ui, Some(icon::LIST_CHECKS), if selecting { "Done selecting" } else { "Select" })
                        {
                            if selecting { stop_select = true } else { start_select = true }
                        }
                        if app.logged_in && !auto && !tracks.is_empty() {
                            any = true;
                        }
                        if owned {
                            if any {
                                widgets::menu_separator(ui);
                            }
                            any = true;
                            if widgets::menu_action(ui, Some(icon::MUSIC), "Edit playlist") {
                                acts.push(Action::EditPlaylist);
                            }
                            if widgets::menu_action(ui, Some(icon::USERS), "Collaborate") {
                                acts.push(Action::Collaborate);
                            }
                            if widgets::menu_action(ui, Some(icon::TRASH_2), "Delete playlist") {
                                acts.push(Action::DeletePlaylist);
                            }
                        }
                        if any {
                            widgets::menu_separator(ui);
                        }
                        if widgets::menu_action(ui, Some(icon::LINK), "Share") {
                            acts.push(Action::Share(target.clone()));
                        }
                        ui.add_space(3.0);
                    });
                    if circle_action(ui, CircleGlyph::Char(icon::SEARCH), "Search in this list", filter_on).clicked() {
                        toggle_search = true;
                    }
                    if let Some(first) = tracks.first()
                        && circle_action(ui, CircleGlyph::Char(icon::RADIO), "Start radio", false).clicked()
                    {
                        acts.push(Action::Radio(first.clone()));
                    }
                });
            });
            if toggle_search {
                let closing = app.coll_search.as_ref().is_some_and(|(fid, _)| fid == id);
                app.coll_search = (!closing).then(|| (id.to_owned(), String::new()));
            }
            if start_select {
                app.selection.start(&app.page);
            }
            if stop_select {
                app.selection.clear();
            }

        });
    });
    let mut dismiss_search = false;
    if let Some((fid, query)) = &mut app.coll_search
        && fid == id
    {
        ui.add_space(14.0);
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(420.0), 40.0), Sense::hover());
        ui.painter().rect_filled(rect, 20.0, theme::glass(0.09));
        widgets::icon_at(ui, pos2(rect.left() + 20.0, rect.center().y), icon::SEARCH, 16.0, theme::text_dim());
        let clearable = !query.is_empty();
        let inner = Rect::from_min_max(pos2(rect.left() + 40.0, rect.top() + 4.0), pos2(rect.right() - if clearable { 40.0 } else { 16.0 }, rect.bottom() - 4.0));
        let edit = ui.put(
            inner,
            egui::TextEdit::singleline(query)
                .hint_text(egui::RichText::new("Search in this list").color(theme::text_faint()))
                .font(theme::regular(14.0))
                .align(egui::Align2::LEFT_CENTER)
                .frame(egui::Frame::NONE)
                .desired_width(inner.width()),
        );
        if toggle_search {
            edit.request_focus();
        } else if !edit.has_focus() && query.is_empty() {
            // Left alone empty, the field goes away instead of sitting over the list.
            dismiss_search = true;
        }
        if clearable {
            let x = Rect::from_center_size(pos2(rect.right() - 20.0, rect.center().y), Vec2::splat(26.0));
            let resp = named(ui.interact(x, ui.scope_id().with("coll-clear"), Sense::click()), "Clear search").on_hover_cursor(egui::CursorIcon::PointingHand);
            widgets::icon_at(ui, x.center(), icon::X, 14.0, theme::text_dim());
            if resp.clicked() {
                query.clear();
            }
        }
        ui.add_space(18.0);
    } else {
        ui.add_space(26.0);
    }
    if dismiss_search {
        app.coll_search = None;
    }
    let needle = app.coll_search.as_ref().filter(|(fid, _)| fid == id).map(|(_, q)| q.trim().to_lowercase()).unwrap_or_default();
    let filtered: Vec<Track> = if needle.is_empty() {
        Vec::new()
    } else {
        c.tracks
            .iter()
            .filter(|t| {
                t.title.to_lowercase().contains(&needle)
                    || t.artist_line().to_lowercase().contains(&needle)
                    || t.album.as_ref().is_some_and(|a| a.name.to_lowercase().contains(&needle))
            })
            .cloned()
            .collect()
    };
    let shown: &[Track] = if needle.is_empty() { tracks } else { &filtered };
    let number = |i: usize, t: &Track| is_album.then(|| t.track_nr.map_or(i + 1, |n| n as usize));
    let own = app.logged_in && c.owned && !is_album;
    list(ui, &rows, &app.liked, shown, !is_album, number, ListCtx { sel: &mut app.selection, page: &app.page, own, plays: top_songs }, acts);
    if more {
        widgets::skeleton_rows(ui, now, 3);
    }
}

// ------------------------------------------------------------------ artist

/// Apple's wordmark for the artist, bottom-left at `base`, no wider than `max_w`.
fn wordmark(ui: &Ui, base: egui::Pos2, logo: &crate::apple::LogoSrc, max_w: f32) -> bool {
    let aspect = logo.w.max(1) as f32 / logo.h.max(1) as f32;
    let mut size = vec2(72.0 * aspect, 72.0);
    if size.x > max_w {
        size = vec2(max_w, max_w / aspect);
    }
    let url = logo.at_height((size.y * 2.0) as u32);
    let rect = Rect::from_min_size(pos2(base.x, base.y - 40.0 - size.y), size);
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    widgets::paint_remote(ui, &url, rect, 0.0, uv, theme::text());
    true
}

fn artist_latest_release_card(ui: &mut Ui, a: &crate::backend::ArtistPage, acts: &mut Vec<Action>, width: f32) {
    let latest = a.albums.first();
    let (card_rect, resp) = ui.allocate_exact_size(vec2(width, 132.0), Sense::click());
    let resp = named(resp, "Latest release").on_hover_cursor(egui::CursorIcon::PointingHand);

    let art_size = 106.0;
    let art_rect = Rect::from_min_size(pos2(card_rect.left() + 2.0, card_rect.center().y - art_size / 2.0), Vec2::splat(art_size));
    let thumbs = latest.map(|l| l.thumbs.as_slice()).unwrap_or_default();
    paint_cover(ui, art_rect, thumbs, 8.0, 1.0);

    let text_x = art_rect.right() + 14.0;
    let text_w = (card_rect.right() - text_x - 12.0).max(40.0);
    let cy = card_rect.center().y;

    // Date: uppercase muted text e.g. "MAY 14, 2026" or year
    let date_str = latest
        .and_then(|l| l.subtitle.split(" • ").map(str::trim).find(|s| s.len() == 4 && s.chars().all(|c| c.is_ascii_digit())))
        .map(|y| format!("RELEASED {y}"))
        .unwrap_or_else(|| "MAY 14, 2026".to_owned());
    ui.painter().text(pos2(text_x, cy - 36.0), Align2::LEFT_TOP, date_str.to_uppercase(), theme::semibold(10.5), theme::text_dim());

    // Title: bold text
    let title = latest.map(|l| l.title.as_str()).unwrap_or("Blossom Tree - Single");
    let tg = widgets::galley_wrapped(ui, title, theme::bold(14.0), theme::text(), text_w, 2);
    ui.painter().galley(pos2(text_x, cy - 14.0), tg, theme::text());

    // Song count: muted text e.g. "1 Song"
    let sub = latest.map(|l| if l.subtitle.contains("Single") { "1 Song" } else { "Album" }).unwrap_or("1 Song");
    ui.painter().text(pos2(text_x, cy + 24.0), Align2::LEFT_TOP, sub, theme::regular(12.0), theme::text_dim());

    if resp.clicked()
        && let Some(l) = latest {
            acts.push(Action::Open(l.target.clone()));
        }
}

fn artist_top_songs_section(ui: &mut Ui, a: &crate::backend::ArtistPage, acts: &mut Vec<Action>, width: f32, liked: &HashSet<String>, visit: u64) {
    let top = &a.top;
    if top.is_empty() {
        return;
    }
    let num_cols = if width >= 650.0 { 3 } else if width >= 420.0 { 2 } else { 1 };
    let rows_per_page = 5;
    let items_per_page = (num_cols * rows_per_page).max(1);
    let total_pages = top.len().div_ceil(items_per_page).max(1);
    let id = format!("artist-top-songs-{}-{}", a.id, visit);
    let (at_start, at_end) = widgets::h_scroll_edges(ui, &id);
    let mut nudge = 0;

    // Header row: "Top Songs" + clickable chevron ">", and on the right the carousel arrows.
    ui.horizontal(|ui| {
        if let Some(pl) = &a.top_playlist {
            let title_g = widgets::galley(ui, "Top Songs", theme::bold(22.0), theme::text(), 400.0);
            let chev_g = widgets::galley(ui, "›", theme::bold(22.0), theme::text_dim(), 40.0);
            let gap = 6.0;
            let title_size = title_g.size();
            let chev_size = chev_g.size();
            let total_w = title_size.x + gap + chev_size.x;
            let total_h = title_size.y.max(chev_size.y);
            let (r, resp) = ui.allocate_exact_size(vec2(total_w, total_h), Sense::click());
            let resp = named(resp, "Show all top songs").on_hover_cursor(egui::CursorIcon::PointingHand);
            let h = resp.hovered();
            let chev_color = if h { theme::text() } else { theme::text_dim() };
            ui.painter().galley(pos2(r.min.x, r.center().y - title_size.y / 2.0), title_g, theme::text());
            ui.painter().galley(pos2(r.min.x + title_size.x + gap, r.center().y - chev_size.y / 2.0), chev_g, chev_color);
            if resp.clicked() {
                acts.push(Action::Open(Target::Playlist(pl.clone())));
            }
        } else {
            ui.label(RichText::new("Top Songs").font(theme::bold(22.0)).color(theme::text()));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            if !at_end {
                let (next_r, next_resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
                let next_resp = named(next_resp, "Next songs").on_hover_cursor(egui::CursorIcon::PointingHand);
                let nh = if next_resp.hovered() { 1.0 } else { 0.0 };
                ui.painter().rect_filled(next_r, 6.0, theme::glass(0.12 + 0.08 * nh));
                ui.painter().rect_stroke(next_r, 6.0, egui::Stroke::new(1.0, theme::glass(0.08)), egui::StrokeKind::Inside);
                ui.painter().text(next_r.center(), Align2::CENTER_CENTER, "›", theme::bold(16.0), theme::text());
                if next_resp.clicked() {
                    nudge = 1;
                }
            }
            if !at_start {
                let (prev_r, prev_resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
                let prev_resp = named(prev_resp, "Previous songs").on_hover_cursor(egui::CursorIcon::PointingHand);
                let ph = if prev_resp.hovered() { 1.0 } else { 0.0 };
                ui.painter().rect_filled(prev_r, 6.0, theme::glass(0.12 + 0.08 * ph));
                ui.painter().rect_stroke(prev_r, 6.0, egui::Stroke::new(1.0, theme::glass(0.08)), egui::StrokeKind::Inside);
                ui.painter().text(prev_r.center(), Align2::CENTER_CENTER, "‹", theme::bold(16.0), theme::text());
                if prev_resp.clicked() {
                    nudge = -1;
                }
            }
        });
    });

    ui.add_space(10.0);

    // Compact song tiles in pages of five rows; the carousel slides smoothly sideways.
    let col_gap = 12.0;
    let col_w = ((width - (num_cols - 1) as f32 * col_gap) / num_cols as f32).max(120.0);
    let tile_h = 46.0;
    let row_gap = 8.0;
    let rows_shown = if total_pages > 1 { rows_per_page } else { top.len().div_ceil(num_cols).clamp(1, rows_per_page) };
    let page_h = rows_shown as f32 * tile_h + rows_shown.saturating_sub(1) as f32 * row_gap;

    widgets::h_scroll(ui, &id, nudge, total_pages, width, 16.0, |ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        for page in 0..total_pages {
            let start = page * items_per_page;
            let page_tracks = &top[start..(start + items_per_page).min(top.len())];
            ui.allocate_ui_with_layout(vec2(width, page_h), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = row_gap;
                for row in 0..rows_shown {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(col_gap, 0.0);
                        for col in 0..num_cols {
                            let idx = row * num_cols + col;
                            if idx < page_tracks.len() {
                                let global_idx = start + idx;
                                let t = &page_tracks[idx];
                                let (tile_rect, resp) = ui.allocate_exact_size(vec2(col_w, tile_h), Sense::click());
                                let resp = named(resp, &t.title).on_hover_cursor(egui::CursorIcon::PointingHand);
                                let h = if resp.hovered() { 1.0 } else { 0.0 };
                                if row % 2 == 0 {
                                    ui.painter().rect_filled(tile_rect, 8.0, theme::glass(0.025));
                                }
                                if h > 0.0 {
                                    ui.painter().rect_filled(tile_rect, 8.0, theme::glass(0.08));
                                }

                                // 40x40 artwork thumbnail
                                let art_rect = Rect::from_center_size(pos2(tile_rect.left() + 23.0, tile_rect.center().y), Vec2::splat(40.0));
                                paint_cover(ui, art_rect, &t.thumbs, 6.0, 1.0);
                                if h > 0.0 {
                                    ui.painter().rect_filled(art_rect, 6.0, theme::shade(0.4));
                                    widgets::play_triangle(ui, art_rect.center(), 6.0, Color32::WHITE);
                                }

                                // Title & Subtitle, with the length kept clear at the right edge.
                                let text_x = art_rect.right() + 10.0;
                                let right_pad = 12.0;
                                let dur_w = if t.duration.is_some() { 36.0 } else { 0.0 };
                                let text_w = (tile_rect.right() - text_x - right_pad - dur_w).max(30.0);
                                let title_g = widgets::galley(ui, &t.title, theme::semibold(13.0), theme::text(), text_w);
                                ui.painter().galley(pos2(text_x, tile_rect.center().y - 14.0), title_g, theme::text());

                                let mut sub = t.album.as_ref().map(|a| a.name.clone()).unwrap_or_else(|| t.artist_line());
                                if let Some(p) = t.plays.map(crate::backend::compact) {
                                    sub = if sub.is_empty() { format!("{p} plays") } else { format!("{sub} \u{2022} {p} plays") };
                                }
                                let sub_g = widgets::galley(ui, &sub, theme::regular(11.5), theme::text_dim(), text_w);
                                ui.painter().galley(pos2(text_x, tile_rect.center().y + 1.0), sub_g, theme::text_dim());

                                if let Some(d) = t.duration {
                                    ui.painter().text(pos2(tile_rect.right() - right_pad, tile_rect.center().y), Align2::RIGHT_CENTER, super::fmt_time(d), theme::regular(11.5), theme::text_dim());
                                }

                                if resp.clicked() {
                                    acts.push(Action::Play(top.clone(), global_idx));
                                }
                                egui::Popup::context_menu(&resp).show(|ui| {
                                    widgets::song_menu(ui, t, liked.contains(&t.id), None, None, acts);
                                });
                            } else {
                                ui.allocate_space(vec2(col_w, tile_h));
                            }
                        }
                    });
                }
            });
        }
    });
}



pub fn artist(app: &mut App, ui: &mut Ui, id: &str, now: f64) {
    if app.artist.as_ref().is_none_or(|a| a.id != id) {
        placeholder_artist(app, ui, now);
        return;
    }
    let accent = app.accent_color();
    // Apple's own page for this artist: their display name and the hero clip.
    let apple = app.apple_artists.get(id).and_then(|o| o.clone());
    let name = apple.as_ref().map(|x| x.name.clone()).unwrap_or_else(|| app.artist.as_ref().map(|a| a.name.clone()).unwrap_or_default());
    if apple.is_none() {
        let key = format!("artist:{id}");
        let cooling = app.apple_missed.get(&key).is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(120));
        if !cooling && app.apple_waiting.insert(key) {
            app.backend.request(crate::backend::Req::AppleArtist { id: id.to_owned(), name: name.clone() });
        }
    }
    let signature = app.signature_texture(ui.ctx(), &name);
    let page_bg = super::shell::base_color(app);
    let Some(a) = &app.artist else { return };
    let acts = &mut app.actions;
    let top = &a.top;

    // Full-width cinematic hero banner fading into the page.
    let w = ui.available_width();
    let hero_h = hero_height(w);
    let (hero, _) = ui.allocate_exact_size(vec2(w, hero_h), Sense::hover());
    let target_px = ((w * ui.ctx().pixels_per_point()).round() as u32).min(1920);
    if let Some(t) = crate::model::pick_thumb(&a.banner, target_px).or_else(|| a.banner.iter().max_by_key(|t| t.width)) {
        let img_ratio = if t.height > 0 { t.width as f32 / t.height as f32 } else { 16.0 / 9.0 };
        let vis_h = (img_ratio / (w / hero_h)).min(1.0);
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, vis_h));
        widgets::paint_remote(ui, &t.url, hero, 0.0, uv, Color32::WHITE);
    } else {
        ui.painter().rect_filled(hero, 0.0, theme::with_alpha(accent, 0.25));
    }
    // Smooth multi-stop downward vertical gradient dissolve into page background
    paint_hero_gradient_dissolve(ui.painter(), hero, page_bg);

    let base_x = hero.left() + 28.0;
    let btn_y = hero.bottom() - 36.0;

    // Circle 1: ... (more options)
    let more_rect = Rect::from_center_size(pos2(base_x + 19.0, btn_y), vec2(38.0, 38.0));
    let more_resp = named(ui.interact(more_rect, ui.scope_id().with("hero_more"), Sense::click()), "Artist options").on_hover_cursor(egui::CursorIcon::PointingHand);
    let more_h = more_resp.hovered();
    ui.painter().circle_filled(more_rect.center(), 19.0, theme::glass(0.15 + 0.08 * (more_h as u8 as f32)));
    ui.painter().circle_stroke(more_rect.center(), 19.0, egui::Stroke::new(1.0, theme::glass(0.12)));
    widgets::paint_more_dots(ui.painter(), more_rect.center(), 4.8, 1.8, theme::text());
    egui::Popup::menu(&more_resp).show(|ui| {
        ui.set_width(200.0);
        if let Some(first) = top.first()
            && widgets::menu_action(ui, Some(icon::RADIO), "Start radio")
        {
            acts.push(Action::Radio(first.clone()));
        }
        if widgets::menu_action(ui, Some(icon::LINK), "Share artist") {
            acts.push(Action::Share(Target::Artist(id.to_owned())));
        }
    });

    // Circle 2: Large circular Play/Pause button with red accent
    let gap = 14.0;
    let play_center = pos2(base_x + 38.0 + gap + 24.0, btn_y);
    let play_rect = Rect::from_center_size(play_center, vec2(48.0, 48.0));
    let artist_target = Target::Artist(id.to_owned());
    let active = widgets::active_state(ui, &artist_target);
    let playing = active == Some(true);
    let play_name = if playing { "Pause artist" } else { "Play artist" };
    let play_resp = named(ui.interact(play_rect, ui.scope_id().with("hero_play"), Sense::click()), play_name).on_hover_cursor(egui::CursorIcon::PointingHand);
    let play_h = play_resp.hovered();
    let play_bg = if play_h { Color32::from_rgb(0xFA, 0x48, 0x5E) } else { theme::RED };
    ui.painter().circle_filled(play_center, 24.0, play_bg);
    widgets::play_pause_glyph(ui, play_center, 8.5, Color32::WHITE, playing);
    if play_resp.clicked() {
        if active.is_some() {
            acts.push(Action::TogglePlay);
        } else if !top.is_empty() {
            acts.push(Action::Play(top.clone(), 0));
        }
    }

    // Circle 3: the account's YouTube subscription to this artist, as a pill: the same
    // subscription the YouTube Music app shows, so it stays in step with it.
    if let Some(sub) = &a.following {
        let label = if sub.subscribed { "Subscribed" } else { "Subscribe" };
        let galley = ui.painter().layout_no_wrap(label.to_owned(), theme::semibold(13.5), theme::text());
        let check_w = if sub.subscribed { 20.0 } else { 0.0 };
        let w = galley.size().x + check_w + 32.0;
        let pill = Rect::from_min_size(pos2(play_center.x + 24.0 + gap, btn_y - 17.0), vec2(w, 34.0));
        let resp = named(ui.interact(pill, ui.scope_id().with("hero_sub"), Sense::click()), label).on_hover_cursor(egui::CursorIcon::PointingHand);
        let h = resp.hovered() as u8 as f32;
        ui.painter().rect_filled(pill, 17.0, theme::glass(0.14 + 0.09 * h));
        ui.painter().rect_stroke(pill, 17.0, egui::Stroke::new(1.0, theme::glass(0.16 + 0.08 * h)), egui::StrokeKind::Inside);
        let mut x = pill.left() + 16.0;
        if sub.subscribed {
            widgets::icon_at(ui, pos2(x + 4.0, btn_y), icon::CHECK, 14.0, theme::text());
            x += check_w;
        }
        ui.painter().galley(pos2(x, btn_y - galley.size().y / 2.0), galley, theme::text());
        if resp.clicked() {
            acts.push(Action::SetSubscribed(!sub.subscribed));
        }
    }

    // Artist name: show special Apple wordmark logo if available, otherwise large bold typography
    let has_logo = apple.as_ref().and_then(|x| x.logo.as_ref()).is_some();
    if let Some(logo) = apple.as_ref().and_then(|x| x.logo.as_ref()) {
        let logo_base = hero.left_bottom() + vec2(28.0, -54.0);
        wordmark(ui, logo_base, logo, w * 0.62);
    } else {
        let name_pos = pos2(base_x, btn_y - 54.0);
        ui.painter().text(name_pos, Align2::LEFT_BOTTOM, &name, theme::bold(52.0), theme::text());
    }
    // Monthly audience and subscribers, as YouTube Music writes them.
    let stats = [a.audience.as_deref(), a.subscribers.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" • ");
    if !stats.is_empty() {
        ui.painter().text(pos2(base_x, btn_y - 30.0), Align2::LEFT_BOTTOM, stats, theme::regular(13.0), theme::text_dim());
    }

    // Content sections below the hero banner, padded to match the rest of the app.
    egui::Frame::new().inner_margin(egui::Margin { left: 28, right: 48, top: 0, bottom: 0 }).show(ui, |ui| {
        let cw = ui.available_width();
        ui.add_space(26.0);
        let is_wide = cw >= 780.0;
        if is_wide {
            let left_w = (cw * 0.30).clamp(280.0, 320.0);
            let right_w = (cw - left_w - 32.0).max(360.0);
            ui.horizontal_top(|ui| {
                // Left Column: Latest Release card
                ui.allocate_ui_with_layout(vec2(left_w, 0.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    ui.label(RichText::new("Latest Release").font(theme::bold(18.0)).color(theme::text()));
                    ui.add_space(8.0);
                    artist_latest_release_card(ui, a, acts, left_w);
                });
                ui.add_space(24.0);
                // Right Column: Top Songs 3-column grid with carousel pagination
                ui.allocate_ui_with_layout(vec2(right_w, 0.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    artist_top_songs_section(ui, a, acts, right_w, &app.liked, app.page_visit);
                });
            });
        } else {
            // Graceful responsive collapse on narrow window
            ui.label(RichText::new("Latest Release").font(theme::bold(18.0)).color(theme::text()));
            ui.add_space(8.0);
            artist_latest_release_card(ui, a, acts, cw.min(420.0));
            ui.add_space(20.0);
            artist_top_songs_section(ui, a, acts, cw, &app.liked, app.page_visit);
        }

        ui.add_space(24.0);
        let id_albums = format!("a-albums-{}", app.page_visit);
        card_row(ui, &id_albums, "Albums & singles", &a.albums, acts);
        let id_pl = format!("a-pl-{}", app.page_visit);
        card_row(ui, &id_pl, "Featured on", &a.playlists, acts);
        let id_sim = format!("a-sim-{}", app.page_visit);
        card_row(ui, &id_sim, "Fans might also like", &a.similar, acts);
        if let Some(d) = a.description.as_deref().filter(|d| !d.trim().is_empty()) {
            shelf_header(ui, "", "About", None, None, false);
            let full = ui.available_width().min(820.0);
            let beside = has_logo.then_some(signature.as_ref()).flatten();
            if let Some(tex) = beside {
                ui.horizontal_top(|ui| {
                    let sig_w = (full * 0.28).clamp(120.0, 210.0);
                    let text_w = (full - sig_w - 32.0).max(220.0);
                    let g = widgets::galley_wrapped(ui, d.trim(), theme::regular(14.5), theme::text_dim(), text_w, usize::MAX);
                    ui.label(g);
                    ui.add_space(32.0);
                    let tex_size = tex.size_vec2();
                    let aspect = tex_size.x / tex_size.y.max(1.0);
                    let mut size = vec2(sig_w, sig_w / aspect);
                    if size.y > 140.0 {
                        size = vec2(140.0 * aspect, 140.0);
                    }
                    let (rect, _) = ui.allocate_exact_size(vec2(size.x, size.y + 22.0), Sense::hover());
                    ui.painter().text(rect.left_top(), Align2::LEFT_TOP, format!("Signed by {name}"), theme::regular(12.0), theme::text_dim());
                    let img = Rect::from_min_size(pos2(rect.left(), rect.top() + 22.0), size);
                    let tex = egui::load::SizedTexture::from_handle(tex);
                    egui::Image::from_texture(tex).uv(Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))).paint_at(ui, img);
                });
            } else {
                let g = widgets::galley_wrapped(ui, d.trim(), theme::regular(14.5), theme::text_dim(), full, usize::MAX);
                ui.label(g);
            }
        }
    });
}

// ------------------------------------------------------------------ library

pub(crate) const HISTORY_ARTISTS_LIMIT: usize = 10;

/// The artists behind the songs in recently played, newest first, each once.
pub(crate) fn played_artists(history: &[Track]) -> Vec<crate::model::Link> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for t in history {
        for a in &t.artists {
            let Some(id) = a.id.clone() else { continue };
            if !seen.insert(id.clone()) {
                continue;
            }
            out.push(crate::model::Link { name: a.name.clone(), id: Some(id) });
        }
    }
    out
}

fn library(app: &mut App, ui: &mut Ui, tab: LibTab, now: f64) {
    page_title(ui, "Library");
    if !app.logged_in {
        ui.add_space(10.0);
        ui.label(RichText::new("Sign in to see your playlists, liked songs and history.").color(theme::text_dim()));
        ui.add_space(12.0);
        if pill(ui, "Sign in with Google", Some(icon::CIRCLE_USER), true).clicked() {
            app.start_sign_in(ui.ctx());
        }
        return;
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for (t, label) in [(LibTab::Playlists, "Playlists"), (LibTab::Artists, "Artists"), (LibTab::Albums, "Albums"), (LibTab::Songs, "Liked songs"), (LibTab::History, "Recently played")] {
            if chip(ui, label, tab == t).clicked() {
                // Tabs swap in place (no history entry); the current one refreshes.
                if tab == t {
                    app.refresh();
                } else {
                    app.page = Page::Library(t);
                    app.page_loaded();
                }
            }
        }
    });
    ui.add_space(12.0);
    let grid = |ui: &mut Ui, cards: &[Card], acts: &mut Vec<Action>| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(20.0, 22.0);
            for c in cards {
                card(ui, c, CARD, false, acts);
            }
        });
    };
    let rows = Rows::new(app, now);
    let plain = |_: usize, _: &Track| None;
    match tab {
        LibTab::History => {
            if app.history.is_none() {
                placeholder_track_list(app, ui, now, false);
                return;
            }
            let Some(h) = &app.history else { return };
            list(ui, &rows, &app.liked, h, true, plain, ListCtx { sel: &mut app.selection, page: &app.page, own: false, plays: false }, &mut app.actions);
        }
        LibTab::Artists => {
            section_label(ui, "Subscribed artists", None);
            if app.library_artists.is_none() {
                placeholder_grid(app, ui, now, CARD, true, 12, false);
                return;
            }
            let followed = app.library_artists.as_deref().unwrap_or_default();
            if followed.is_empty() {
                ui.label(RichText::new("Artists you follow show up here.").color(theme::text_dim()));
            } else {
                grid(ui, followed, &mut app.actions);
            }
            // The artists behind the songs in History, newest first (limited to latest 10).
            let played = app.history.as_deref().map(played_artists).unwrap_or_default();
            if !played.is_empty() {
                section_label(ui, "From your history", None);
                let played_cards: Vec<Card> = played
                    .into_iter()
                    .take(HISTORY_ARTISTS_LIMIT)
                    .filter_map(|a| {
                        let id = a.id?;
                        let thumbs = app.artist_photos.get(&id).cloned().filter(|t| !t.is_empty())
                            .or_else(|| followed.iter().find(|c| matches!(&c.target, Target::Artist(cid) if cid == &id)).map(|c| c.thumbs.clone()))
                            .or_else(|| app.artists_cache.get(&id).map(|(a, _)| if a.photo.is_empty() { a.banner.clone() } else { a.photo.clone() }))
                            .or_else(|| app.artist.as_ref().filter(|a| a.id == id).map(|a| if a.photo.is_empty() { a.banner.clone() } else { a.photo.clone() }))
                            .unwrap_or_default();
                        if thumbs.is_empty() && !app.artist_photos.contains_key(&id) {
                            app.artist_photos.insert(id.clone(), Vec::new());
                            app.backend.request(crate::backend::Req::ArtistPhoto(id.clone()));
                        }
                        Some(Card {
                            title: a.name,
                            subtitle: String::new(),
                            thumbs,
                            target: Target::Artist(id),
                            round: true,
                            links: Vec::new(),
                        })
                    })
                    .collect();
                grid(ui, &played_cards, &mut app.actions);
                ui.add_space(10.0);
            }
        }
        // Liked Music, kept like any playlist.
        LibTab::Songs => {
            if app.opened(super::LIKED_MUSIC).is_none() {
                placeholder_track_list(app, ui, now, true);
                return;
            }
            let more = app.receiving(super::LIKED_MUSIC);
            let Some(o) = app.lists.get(super::LIKED_MUSIC) else { return };
            let liked = &o.c.tracks;
            if !liked.is_empty() {
                ui.horizontal(|ui| {
                    play_pill(ui, &mut app.actions, &Target::Playlist(super::LIKED_MUSIC.into()), liked);
                    if pill(ui, &t!("shuffle"), Some(icon::SHUFFLE), false).clicked() {
                        let mut t = liked.clone();
                        super::shuffle(&mut t);
                        app.actions.push(Action::Play(t, 0));
                    }
                    select_pill(ui, &mut app.selection, &app.page);
                });
                ui.add_space(10.0);
            }
            list(ui, &rows, &app.liked, liked, true, plain, ListCtx { sel: &mut app.selection, page: &app.page, own: false, plays: false }, &mut app.actions);
            if more {
                widgets::skeleton_rows(ui, now, 3);
            }
        }
        _ => {
            if app.library.is_none() {
                let with_new = tab == LibTab::Playlists;
                placeholder_grid(app, ui, now, CARD, false, 12, with_new);
                return;
            }
            let Some(lib) = &app.library else { return };
            match tab {
                LibTab::Playlists => {
                    if pill(ui, "New playlist", None, false).clicked() {
                        app.actions.push(Action::NewPlaylist(Vec::new()));
                    }
                    ui.add_space(14.0);
                    grid(ui, &lib.playlists, &mut app.actions)
                }
                LibTab::Albums => grid(ui, &lib.albums, &mut app.actions),
                LibTab::Songs | LibTab::History | LibTab::Artists => unreachable!(),
            }
        }
    }
}

// ------------------------------------------------------------------ downloads

fn downloads(app: &mut App, ui: &mut Ui, now: f64) {
    page_title(ui, "Downloads");
    let songs = crate::downloads::list();
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("{} songs", songs.len()))
            .font(theme::regular(13.0))
            .color(theme::text_dim()),
    );
    ui.add_space(12.0);
    if songs.is_empty() {
        ui.label(RichText::new("No downloads yet").font(theme::semibold(16.0)).color(theme::text()));
        ui.add_space(10.0);
        if pill(ui, "Open folder", Some(icon::FOLDER_OPEN), false).clicked() {
            crate::downloads::open_folder();
        }
        return;
    }
    ui.horizontal(|ui| {
        if pill(ui, &t!("play"), Some(icon::PLAY), true).clicked() {
            app.actions.push(Action::Play(songs.clone(), 0));
        }
        if pill(ui, &t!("shuffle"), Some(icon::SHUFFLE), false).clicked() {
            let mut t = songs.clone();
            super::shuffle(&mut t);
            app.actions.push(Action::Play(t, 0));
        }
        if pill(ui, "Open folder", Some(icon::FOLDER_OPEN), false).clicked() {
            crate::downloads::open_folder();
        }
    });
    ui.add_space(12.0);
    let rows = Rows::new(app, now);
    list(ui, &rows, &app.liked, &songs, true, |_, _| None, ListCtx { sel: &mut app.selection, page: &app.page, own: false, plays: false }, &mut app.actions);
}

// ------------------------------------------------------------------ settings

fn settings(app: &mut App, ui: &mut Ui) {
    // One centred column; every section shares its width.
    let avail = ui.available_width();
    let w = avail.min(920.0);
    ui.horizontal(|ui| {
        ui.add_space(((avail - w) / 2.0).max(0.0));
        ui.vertical(|ui| {
            ui.set_width(w);
            settings_body(app, ui);
            ui.add_space(40.0);
        });
    });
}

fn settings_body(app: &mut App, ui: &mut Ui) {
    page_title(ui, "Settings");
    settings_account(app, ui);
    settings_appearance(app, ui);
    settings_playback(app, ui);
    settings_audio(app, ui);
    settings_downloads(ui);
    settings_notifications(app, ui);
    settings_window(app, ui);
    settings_connections(app, ui);
    super::transfer::settings_section(app, ui);
    settings_network(app, ui);
    settings_shortcuts(ui);
    settings_updates(app, ui);
}

/// Settings › Account: signing in and out, and which channel to use.
/// Settings › Updates: the version, what the updater is doing, and a button to check
/// now (or restart into a downloaded update).
fn settings_updates(app: &mut App, ui: &mut Ui) {
    use crate::update::State;
    section_label(ui, "Updates", None);
    let version = crate::update::current_version();
    let state = app.updater.as_ref().map(|u| u.state()).unwrap_or(State::Idle);
    let can_update = super::updatable();
    let status = match &state {
        _ if !can_update => "Updates install in the app from the .dmg, opened from Applications.".to_owned(),
        State::Idle => "Dawn FM checks for updates each time it opens.".to_owned(),
        State::Checking => "Checking for updates…".to_owned(),
        State::UpToDate => "You're up to date.".to_owned(),
        State::Downloading { version, percent } => format!("Downloading Dawn FM {version}… {percent}%"),
        State::Ready { version, .. } => format!("Dawn FM {version} is ready. Restart to update."),
        State::Failed(e) => format!("Couldn't check for updates: {e}"),
    };
    let mut check = false;
    let mut restart = false;
    settings_card(ui, |ui| {
        setting_row(ui, &format!("Dawn FM {version}"), Some(&status), |ui| {
            match &state {
                State::Ready { .. } => restart = pill(ui, "Restart to Update", Some(icon::SPARKLES), true).clicked(),
                State::Downloading { percent, .. } => {
                    let progress = (*percent as f32 / 100.0).clamp(0.0, 1.0);
                    ui.add(egui::ProgressBar::new(progress).desired_width(120.0).show_percentage());
                }
                State::Checking => {
                    ui.spinner();
                }
                _ => {
                    let resp = ui.add_enabled_ui(can_update, |ui| pill(ui, "Check for Updates", None, false)).inner;
                    check = resp.clicked() && can_update;
                }
            }
        });
    });
    if check
        && let Some(u) = &app.updater
    {
        u.check();
        app.update_checked = Some(ui.input(|i| i.time));
    }
    if restart {
        app.restart_to_update(ui.ctx());
    }
}

fn settings_account(app: &mut App, ui: &mut Ui) {
    section_label(ui, "Account", None);
    let busy = app.signing_in.load(Ordering::Relaxed);
    let (mut sign_in, mut sign_out) = (false, false);
    // Asked again now and then while Settings shows: a failed ask, or a channel made since.
    let now = ui.input(|i| i.time);
    if app.logged_in && app.channels_asked.is_none_or(|at| now - at > 60.0) {
        app.channels_asked = Some(now);
        app.backend.request(crate::backend::Req::Channels);
    }
    settings_card(ui, |ui| {
        if app.logged_in {
            let name = app.account_name.clone().unwrap_or_else(|| "YouTube Music".to_owned());
            setting_row(ui, &name, Some("Signed in with Google. Your home feed, library and likes come from this account. Saved in your macOS Keychain."), |ui| {
                sign_out = pill_destructive(ui, "Sign out", Some(icon::LOG_OUT)).clicked();
            });
            // Also with one, when the one in use isn't it (a brand channel since deleted).
            if app.channels.as_ref().is_some_and(|c| c.list.len() > 1 || !c.list.iter().any(|l| l.page_id == c.current)) {
                divider(ui);
                setting_row(ui, "Channel", Some("Your Google account and each YouTube channel it manages have their own library, likes, history and recommendations."), |ui| {
                    channel_picker(app, ui);
                });
            }
        } else {
            setting_row(ui, "Not signed in", Some("Sign in to get your home feed, library and likes."), |ui| {
                if busy {
                    ui.spinner();
                } else {
                    sign_in = pill(ui, "Sign in with Google", Some(icon::CIRCLE_USER), true).clicked();
                }
            });
        }
    });
    if busy {
        ui.add_space(6.0);
        ui.label(RichText::new("Finish signing in in the window that opened.").font(theme::regular(11.0)).color(theme::text_dim()));
    }
    if sign_in {
        app.start_sign_in(ui.ctx());
    }
    if sign_out {
        app.backend.request(crate::backend::Req::Logout);
    }
}

fn channel_picker(app: &mut App, ui: &mut Ui) {
    let Some(crate::backend::Channels { list, current, epoch }) = &app.channels else { return };
    // A brand channel often has the account's name: the handle, or "No channel" (as
    // YouTube's switcher says), tells them apart.
    let label = |c: &crate::ytm::Channel| match (&c.handle, c.has_channel) {
        (Some(h), _) => format!("{} ({h})", c.name),
        (None, false) => format!("{} (No channel)", c.name),
        (None, true) => c.name.clone(),
    };
    // The button is narrower: a handle alone is enough there.
    let shown = match list.iter().find(|c| c.page_id == *current) {
        Some(c) => c.handle.clone().unwrap_or_else(|| label(c)),
        None => "Another channel".to_owned(),
    };
    // A transfer would go on as the new channel.
    if app.transfer.running() {
        ui.label(RichText::new("Not while a transfer runs").font(theme::regular(13.0)).color(theme::text_dim()));
        return;
    }
    let switching = app.switching_to.is_some();
    let btn = dropdown_button(ui, if switching { "Switching…" } else { &shown }, 220.0, "Channel");
    let mut pick = None;
    egui::Popup::menu(&btn).show(|ui| {
        ui.set_width(300.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            ui.add_space(4.0);
            for c in list {
                if option_row(ui, &label(c), c.page_id == *current).clicked() {
                    pick = Some(c.clone());
                    ui.close();
                }
            }
            ui.add_space(4.0);
        });
    });
    if let Some(c) = pick.filter(|c| c.page_id != *current && !switching) {
        app.switching_to = Some(label(&c));
        app.backend.request(crate::backend::Req::SwitchChannel { page_id: c.page_id, epoch: *epoch });
    }
}

/// Settings › Playback.
fn settings_playback(app: &mut App, ui: &mut Ui) {
    section_label(ui, "Playback", None);
    settings_card(ui, |ui| {
        setting_row(ui, "Autoplay", Some("Keep playing similar songs when the queue ends."), |ui| {
            toggle(ui, &mut app.autoplay, "Autoplay");
        });
        divider(ui);
        setting_row(ui, "Output device", Some("Where the music plays. A picked device that isn't connected leaves it to the Mac's default until it's back."), |ui| {
            output_device_picker(app, ui);
        });
        divider(ui);
        setting_row(ui, "Play songs, not music videos", Some("An official music video plays as its song, the album recording, when YouTube Music has one: no intros, skits or video-only edits. Lists still show what they hold."), |ui| {
            toggle(ui, &mut app.play_songs, "Play songs, not music videos");
        });
        divider(ui);
        setting_row(ui, "Skip non-music parts", Some("Jump over talking, skits and sponsor breaks in music videos, as marked by the SponsorBlock community. Album songs don't have any."), |ui| {
            if toggle(ui, &mut app.sponsorblock, "Skip non-music parts").changed() {
                app.request_segments();
            }
        });
        divider(ui);
        setting_row(ui, "Automix", Some("Slide from one song into the next the way a DJ would: both hold their level for a moment, then swap in one smooth move, instead of a straight fade. The mix picks its own length, in phrases of 8 or 16 bars (16 on long songs), following the beat it hears."), |ui| {
            if toggle(ui, &mut app.automix, "Automix").changed() {
                app.set_crossfade();
            }
        });
        if !app.automix {
            divider(ui);
            setting_row(ui, "Crossfade songs", Some("Blend the end of each song into the start of the next."), |ui| {
                if toggle(ui, &mut app.crossfade, "Crossfade songs").changed() {
                    app.set_crossfade();
                }
            });
            if app.crossfade {
                divider(ui);
                setting_row(ui, "Crossfade length", None, |ui| {
                    if seconds_slider(ui, &mut app.crossfade_secs, 1..=super::CROSSFADE_MAX, "Crossfade length").changed() {
                        app.set_crossfade();
                    }
                });
            }
        }
    });
}

/// Settings › Audio quality: loudness and where the audio comes from.
fn settings_audio(app: &mut App, ui: &mut Ui) {
    section_label(ui, "Audio quality", None);
    settings_card(ui, |ui| {
        setting_row(ui, "Volume normalization", Some("Turn loud songs down to the same level, like YouTube Music does."), |ui| {
            if toggle(ui, &mut app.normalize, "Volume normalization").changed() {
                app.audio.set_normalize(app.normalize);
            }
        });
        divider(ui);
        setting_row(ui, "Logarithmic volume", Some("Space the volume slider out the way ears hear loudness, so quiet levels aren't squeezed into its first few pixels."), |ui| {
            if toggle(ui, &mut app.log_volume, "Logarithmic volume").changed() {
                app.audio.set_log_volume(app.log_volume);
            }
        });
        divider(ui);
        setting_row(ui, "Hi-Res audio", Some("Addon servers that serve hi-res copies of songs. When one has the song, its copy plays instead of YouTube's, and its quality shows in the player."), |_| {});
        divider(ui);
        ui.add_space(12.0);
        hires_sources_editor(app, ui);
        ui.add_space(10.0);
        divider(ui);
        setting_row(ui, "Prefer Dolby Atmos", Some("Ask the hi-res sources for a Dolby Atmos copy when they offer one. Falls back to the best stereo copy when the stream's codec can't be decoded."), |ui| {
            if toggle(ui, &mut app.prefer_atmos, "Prefer Dolby Atmos").changed() {
                app.backend.set_prefer_atmos(app.prefer_atmos);
            }
        });
    });
}

/// The hi-res source list: one line per addon, and "+ Add a source" below it that
/// opens a fresh line for the next one.
fn hires_sources_editor(app: &mut App, ui: &mut Ui) {
    let mut edited = false;
    let mut remove = None;
    for i in 0..app.hires_sources.len() {
        ui.horizontal(|ui| {
            let field_w = (ui.available_width() - 34.0).max(120.0);
            ui.allocate_ui_with_layout(vec2(field_w, 42.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(field_w);
                if super::text_field(ui, "", &mut app.hires_sources[i], false, "https://your-addon.example").changed() {
                    edited = true;
                }
            });
            if widgets::icon_button(ui, icon::X, 14.0, "Remove source", false).clicked() {
                remove = Some(i);
            }
        });
        ui.add_space(6.0);
    }
    if let Some(i) = remove {
        app.hires_sources.remove(i);
        edited = true;
    }
    if edited {
        app.backend.set_hires_source(&live_sources(app));
    }
    if app.hires_adding {
        let resp = super::text_field(ui, "", &mut app.hires_draft, false, "https://your-addon.example");
        if app.hires_focus {
            resp.request_focus();
            app.hires_focus = false;
        }
        if resp.lost_focus() {
            let link = app.hires_draft.trim().to_owned();
            if !link.is_empty() && !app.hires_sources.iter().any(|s| s.trim() == link) {
                app.hires_sources.push(link);
                app.backend.set_hires_source(&live_sources(app));
            }
            app.hires_draft.clear();
            app.hires_adding = false;
        }
        ui.add_space(6.0);
    } else if pill(ui, "+  Add a source", None, false).clicked() {
        app.hires_adding = true;
        app.hires_focus = true;
        app.hires_draft.clear();
    }
}

/// The non-empty links as one newline-separated list, the way the backend reads them.
fn live_sources(app: &App) -> String {
    app.hires_sources.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n")
}

/// Settings › Downloads.
fn settings_downloads(ui: &mut Ui) {
    section_label(ui, "Downloads", None);
    ui.label(
        RichText::new("Download songs from their menu. They're saved at the best quality your account gets, tagged with their cover art, and play from the file, even offline.")
            .font(theme::regular(11.0))
            .color(theme::text_dim()),
    );
    ui.add_space(12.0);
    settings_card(ui, |ui| {
        let folder = crate::downloads::folder().display().to_string();
        setting_row(ui, "Download folder", Some(&folder), |ui| {
            if pill(ui, "Change", None, false).clicked() {
                crate::downloads::pick_folder(ui.ctx());
            }
            ui.add_space(6.0);
            if pill(ui, "Open", Some(icon::FOLDER_OPEN), false).clicked() {
                crate::downloads::open_folder();
            }
        });
    });
}


/// Settings › Appearance.
fn settings_appearance(app: &mut App, ui: &mut Ui) {
    section_label(ui, "Appearance", None);
    settings_card(ui, |ui| {
        let desc = "Light or dark pages. System follows your system's setting. Now playing stays dark, over the album art.";
        let mut pick = None;
        setting_row(ui, "Mode", Some(desc), |ui| {
            let btn = dropdown_button(ui, app.appearance.label(), 160.0, "Mode");
            egui::Popup::menu(&btn).show(|ui| {
                ui.set_width(160.0);
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                ui.add_space(4.0);
                for a in theme::Appearance::ALL {
                    if option_row(ui, a.label(), a == app.appearance).clicked() {
                        pick = Some(a);
                        ui.close();
                    }
                }
                ui.add_space(4.0);
            });
        });
        if let Some(a) = pick {
            app.appearance = a;
        }
        divider(ui);
        // The built-in look's colour: its accent, and the tint of every page.
        if app.look().id == crate::preset::DEFAULT_ID {
            setting_row(ui, "Colour", Some("The accent, and the tint of the pages behind everything."), |ui| {
                use crate::preset::COLOURS;
                const SIDE: f32 = 20.0;
                const GAP: f32 = 5.0;
                let (row, _) = ui.allocate_exact_size(vec2(COLOURS.len() as f32 * (SIDE + GAP) - GAP, SIDE + 8.0), Sense::hover());
                let current = crate::preset::colour().id;
                for (i, c) in COLOURS.iter().enumerate() {
                    let r = Rect::from_center_size(pos2(row.left() + SIDE / 2.0 + i as f32 * (SIDE + GAP), row.center().y), Vec2::splat(SIDE));
                    let resp = ui.interact(r.expand(GAP / 2.0), ui.scope_id().with(("colour", c.id)), Sense::click());
                    let resp = widgets::named(resp, c.name).on_hover_text(c.name).on_hover_cursor(egui::CursorIcon::PointingHand);
                    let h = if resp.hovered() { 1.0 } else { 0.0 };
                    let on = theme::anim_bool(ui.ctx(), resp.id.with("on"), c.id == current, 0.15);
                    let fill = crate::preset::hex(c.accent).unwrap_or(theme::RED);
                    ui.painter().circle_filled(r.center(), SIDE / 2.0 - 2.0 + h, fill);
                    if on > 0.0 {
                        ui.painter().circle_stroke(r.center(), SIDE / 2.0 + 1.5, egui::Stroke::new(1.5, theme::with_alpha(theme::text(), on)));
                    }
                    if resp.clicked() {
                        crate::preset::set_colour(c.id);
                        // egui's own colours (the text selection) follow.
                        theme::apply_chrome(ui.ctx(), app.look());
                    }
                }
            });
            divider(ui);
        }

        setting_row(ui, "Size", Some("Everything in the app, larger or smaller. ⌘+ and ⌘− do the same from anywhere, and ⌘0 goes back to 100%."), |ui| {
            // Right to left: larger, the size (a click resets it), smaller.
            let ctx = ui.ctx().clone();
            let z = ctx.zoom_factor();
            let step = |ui: &mut Ui, up: bool, on: bool| -> bool {
                let name = if up { "Larger" } else { "Smaller" };
                let (r, resp) = ui.allocate_exact_size(vec2(32.0, 30.0), if on { Sense::click() } else { Sense::hover() });
                let resp = widgets::named(resp, name).on_hover_text(name);
                let h = if on && resp.hovered() { 1.0 } else { 0.0 };
                ui.painter().rect_filled(r, 8.0, theme::glass(0.08 + 0.08 * h));
                let stroke = egui::Stroke::new(1.6, theme::with_alpha(theme::text(), if on { 1.0 } else { 0.35 }));
                ui.painter().hline((r.center().x - 5.0)..=(r.center().x + 5.0), r.center().y, stroke);
                if up {
                    ui.painter().vline(r.center().x, (r.center().y - 5.0)..=(r.center().y + 5.0), stroke);
                }
                resp.clicked()
            };
            ui.spacing_mut().item_spacing.x = 6.0;
            if step(ui, true, z < *super::shell::ZOOM_RANGE.end() - 0.001) {
                super::shell::zoom_step(&ctx, true);
            }
            let (r, resp) = ui.allocate_exact_size(vec2(52.0, 30.0), Sense::click());
            let resp = widgets::named(resp, "Reset size").on_hover_text("Back to 100%").on_hover_cursor(egui::CursorIcon::PointingHand);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, format!("{:.0}%", z * 100.0), theme::semibold(13.5), theme::text());
            if resp.clicked() {
                ctx.set_zoom_factor(1.0);
            }
            if step(ui, false, z > *super::shell::ZOOM_RANGE.start() + 0.001) {
                super::shell::zoom_step(&ctx, false);
            }
        });
    });
}

/// Settings › Notifications: what a song change says on the desktop.
fn settings_notifications(app: &mut App, ui: &mut Ui) {
    section_label(ui, "Notifications", None);
    settings_card(ui, |ui| {
        setting_row(ui, "Desktop notifications", Some("Show a macOS notification when a new song starts while the window isn't in front."), |ui| {
            toggle(ui, &mut app.notify_songs, "Desktop notifications");
        });
    });
}

/// Settings › Window.
fn settings_window(app: &mut App, ui: &mut Ui) {
    section_label(ui, "Window", None);
    settings_card(ui, |ui| {
        let api = crate::renderer::api_name();
        let title = format!("Draw with {api}");
        let desc = format!("Use {api} instead of OpenGL. Turn this on if graphics glitch or display incorrectly.");
        setting_row(ui, &title, Some(&desc), |ui| {
            if toggle(ui, &mut app.wgpu_renderer, &title).changed() {
                use crate::renderer::Renderer;
                let r = if app.wgpu_renderer { Renderer::Wgpu } else { Renderer::OpenGl };
                if let Err(e) = crate::renderer::save(app.backend.data_dir(), r) {
                    log::warn!("renderer: couldn't save the setting: {e}");
                }
            }
        });
        if app.wgpu_renderer != (crate::renderer::active() == crate::renderer::Renderer::Wgpu) {
            divider(ui);
            let mut restart = false;
            setting_row(ui, "Restart to apply", Some("The app draws with the new renderer from the next start."), |ui| {
                restart = pill(ui, "Restart now", None, true).clicked();
            });
            if restart {
                app.restart(ui.ctx());
            }
        }
    });
}

/// Settings › Connections.
fn settings_connections(app: &mut App, ui: &mut Ui) {
    let mut discord_enabled = app.discord_enabled;
    section_label(ui, "Connections", None);
    settings_card(ui, |ui| {
        setting_row(ui, "Discord", Some("Show what you're playing on your Discord profile."), |ui| {
            toggle(ui, &mut discord_enabled, "Discord");
        });
        divider(ui);
        let busy = app.lastfm_busy.load(Ordering::Relaxed);
        let desc = match &app.lastfm_user {
            Some(name) => format!("Scrobbling what you play to {name}'s profile."),
            None if busy => "Allow Dawn FM on the Last.fm page that opened in your browser.".to_owned(),
            None => "Scrobble what you play to your Last.fm profile.".to_owned(),
        };
        let (mut connect, mut disconnect) = (false, false);
        setting_row(ui, "Last.fm", Some(&desc), |ui| {
            if app.lastfm_user.is_some() {
                disconnect = pill(ui, "Disconnect", None, false).clicked();
            } else if busy {
                ui.spinner();
            } else {
                connect = pill(ui, "Connect", None, false).clicked();
            }
        });
        if connect {
            app.lastfm_sign_in();
        }
        if disconnect {
            app.lastfm_sign_out();
        }
    });
    if discord_enabled != app.discord_enabled {
        app.set_discord_enabled(discord_enabled);
    }
}

/// Settings › Keyboard shortcuts.
fn settings_shortcuts(ui: &mut Ui) {
    let open = section_fold(ui, "Keyboard shortcuts");
    fold_body(ui, fold_state(ui, "Keyboard shortcuts").0, open, |ui| settings_card(ui, |ui| {
        let keys: &[(&str, &[&str])] = &[
            ("Play / pause", &["Space"]),
            ("Next / previous song", &["⌘", "→ / ←"]),
            ("Volume up / down", &["⌘", "↑ / ↓"]),
            ("Search", &["⌘", "F"]),
            ("Lyrics panel", &["⌘", "L"]),
            ("Settings", &["⌘", ","]),
            ("Larger / smaller", &["⌘", "+ / −"]),
            ("Actual size", &["⌘", "0"]),
            ("Mini player", &["⌥", "⌘", "M"]),
            ("Lyrics in now playing", &["L"]),
            ("Queue", &["Q"]),
            ("Full-screen lyrics (in now playing)", &["F"]),
            ("Back / forward", &["⌥", "← / →"]),
            ("Close now playing", &["Esc"]),
        ];
        for (i, (what, combo)) in keys.iter().enumerate() {
            if i > 0 {
                divider(ui);
            }
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
            ui.painter().text(pos2(rect.left(), rect.center().y), Align2::LEFT_CENTER, *what, theme::regular(14.0), theme::text());
            let mut x = rect.right();
            for (j, k) in combo.iter().enumerate().rev() {
                x = keycap(ui, pos2(x, rect.center().y), k) - 6.0;
                if j > 0 {
                    let plus = ui.painter().text(pos2(x, rect.center().y), Align2::RIGHT_CENTER, "+", theme::regular(13.0), theme::text_dim());
                    x = plus.left() - 6.0;
                }
            }
        }
    }));
}

/// Settings › Network: collapsible section for proxy configuration and connection testing.
fn settings_network(app: &mut App, ui: &mut Ui) {
    let open = section_fold(ui, "Network");
    fold_body(ui, fold_state(ui, "Network").0, open, |ui| settings_card(ui, |ui| network(app, ui)));
}

/// Settings › Network: the proxy everything goes through, a connection test, and a
/// restart when the setting changed.
fn network(app: &mut App, ui: &mut Ui) {
    use crate::proxy::{self, Setting};
    let same = |a: &Setting, b: &Setting| std::mem::discriminant(a) == std::mem::discriminant(b);
    let mut pick = None;
    setting_row(ui, "Proxy", Some("Send everything Dawn FM fetches through a proxy: songs, lyrics, sign-in and Listen together."), |ui| {
        let btn = dropdown_button(ui, app.proxy_mode.label(), 220.0, "Proxy");
        egui::Popup::menu(&btn).show(|ui| {
            ui.set_width(220.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.add_space(4.0);
            for s in [Setting::System, Setting::Off, Setting::Custom(String::new())] {
                if option_row(ui, s.label(), same(&s, &app.proxy_mode)).clicked() {
                    pick = Some(s);
                    ui.close();
                }
            }
            ui.add_space(4.0);
        });
    });
    if let Some(s) = pick {
        app.proxy_error = None;
        *app.proxy_test.lock_unpoisoned() = None;
        app.proxy_mode = s.clone();
        // System and Off take effect as picked; a custom address once it's saved.
        if !matches!(s, Setting::Custom(_)) {
            match proxy::save(app.backend.data_dir(), &s) {
                Ok(()) => app.proxy_saved = s,
                Err(e) => app.proxy_error = Some(format!("{e:#}")),
            }
        }
    }
    match &app.proxy_mode {
        Setting::System => {
            divider(ui);
            let desc = match proxy::system() {
                Some(p) => format!("Using {}.", proxy::redact(&p)),
                None => "Dawn FM uses the HTTPS_PROXY or HTTP_PROXY environment variable when one is set, and otherwise connects directly.".to_owned(),
            };
            setting_row(ui, "System proxy", Some(&desc), |_| {});
        }
        Setting::Custom(_) => {
            divider(ui);
            ui.add_space(12.0);
            super::text_field(ui, "Address", &mut app.proxy_url, false, "http://127.0.0.1:8080 or socks5://user:pass@host:1080");
            ui.add_space(10.0);
            let custom = Setting::Custom(app.proxy_url.trim().to_owned());
            let saved = app.proxy_saved == custom;
            let mut save = false;
            let save_label = if saved { t!("saved") } else { t!("save") };
            ui.horizontal(|ui| {
                save = pill(ui, &save_label, None, !saved).clicked() && !saved;
                ui.add_space(8.0);
                ui.label(RichText::new("The address, password included, is kept in your macOS Keychain.").font(theme::regular(12.5)).color(theme::text_dim()));
            });
            if save {
                match proxy::save(app.backend.data_dir(), &custom) {
                    Ok(()) => {
                        app.proxy_saved = custom;
                        app.proxy_error = None;
                    }
                    Err(e) => app.proxy_error = Some(format!("{e:#}")),
                }
            }
            ui.add_space(12.0);
        }
        Setting::Off => {}
    }
    if let Some(e) = &app.proxy_error {
        ui.label(RichText::new(e).font(theme::regular(12.5)).color(theme::RED));
        ui.add_space(8.0);
    }
    divider(ui);
    let result = app.proxy_test.lock_unpoisoned().clone();
    let desc = match &result {
        Some(Some((_, msg))) => msg.clone(),
        Some(None) => "Checking…".to_owned(),
        None => "Checks that YouTube Music answers through this setting.".to_owned(),
    };
    setting_row(ui, "Test connection", Some(&desc), |ui| {
        if matches!(result, Some(None)) {
            ui.spinner();
        } else if pill(ui, "Test", None, false).clicked() {
            let s = match &app.proxy_mode {
                Setting::Custom(_) => Setting::Custom(app.proxy_url.trim().to_owned()),
                other => other.clone(),
            };
            let (slot, b) = (app.proxy_test.clone(), app.backend.clone());
            *slot.lock_unpoisoned() = Some(None);
            app.backend.spawn(async move {
                let out = match proxy::test(&s).await {
                    Ok(t) => (true, format!("Working: YouTube Music answered in {} ms.", t.as_millis())),
                    Err(e) => (false, format!("Didn't connect: {e:#}")),
                };
                *slot.lock_unpoisoned() = Some(Some(out));
                b.wake();
            });
        }
    });
    if app.proxy_saved != *proxy::active() {
        divider(ui);
        let mut restart = false;
        setting_row(ui, "Restart to apply", Some("The new proxy setting is used from the next start."), |ui| {
            restart = pill(ui, "Restart now", None, true).clicked();
        });
        if restart {
            app.restart(ui.ctx());
        }
    }
}

const SYSTEM_DEFAULT: &str = "System default";

fn output_device_picker(app: &mut App, ui: &mut Ui) {
    let current = match &app.output_device {
        None => SYSTEM_DEFAULT.to_owned(),
        Some((id, name)) => match app.output_devices.iter().find(|d| d.id == *id) {
            Some(d) => d.name.clone(),
            None if app.output_devices.is_empty() => name.clone(),
            None => format!("{name} (not connected)"),
        },
    };
    let btn = dropdown_button(ui, &current, 210.0, "Output device");
    // Listed when opened, so a device that just connected shows up.
    if btn.clicked() || app.output_devices.is_empty() {
        app.output_devices = crate::audio::output_devices();
    }
    let mut pick: Option<Option<(String, String)>> = None;
    egui::Popup::menu(&btn).show(|ui| {
        ui.set_width(340.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            ui.add_space(4.0);
            if option_row(ui, SYSTEM_DEFAULT, app.output_device.is_none()).clicked() {
                pick = Some(None);
                ui.close();
            }
            for d in &app.output_devices {
                if option_row(ui, &d.name, app.output_device.as_ref().is_some_and(|(id, _)| *id == d.id)).clicked() {
                    pick = Some(Some((d.id.clone(), d.name.clone())));
                    ui.close();
                }
            }
            ui.add_space(4.0);
        });
    });
    if let Some(pick) = pick {
        app.audio.send(crate::audio::AudioCmd::SetDevice(pick.as_ref().map(|(id, _)| id.clone())));
        app.output_device = pick;
    }
}

/// A dropdown's closed state: the chosen value and a chevron on a glass pill.
fn dropdown_button(ui: &mut Ui, value: &str, width: f32, name: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 36.0), Sense::click());
    let resp = widgets::named(resp, name).on_hover_cursor(egui::CursorIcon::PointingHand);
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    let h = if resp.hovered() || open { 1.0 } else { 0.0 };
    ui.painter().rect_filled(rect, 10.0, theme::glass(0.08 + 0.05 * h));
    widgets::text_at(ui, pos2(rect.left() + 14.0, rect.center().y - 9.0), value, theme::regular(14.0), theme::text(), rect.width() - 44.0);
    let glyph = if open { theme::icon::CHEVRON_UP } else { theme::icon::CHEVRON_DOWN };
    widgets::icon_at(ui, pos2(rect.right() - 18.0, rect.center().y), glyph, 15.0, theme::lerp_color(theme::text_dim(), theme::text(), h));
    resp
}

/// A dropdown option: macOS menu item style with theme accent hover and leading check.
fn option_row(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 21.0), Sense::click());
    let resp = widgets::named(resp, label);
    let over = resp.hovered();
    if over {
        ui.painter().rect_filled(rect, 4.0, app_accent(ui));
    }
    let fg = if over {
        Color32::WHITE
    } else if theme::light() {
        Color32::from_rgb(32, 32, 32)
    } else {
        Color32::from_rgb(235, 235, 235)
    };
    if selected {
        widgets::icon_at(ui, pos2(rect.left() + 12.0, rect.center().y), theme::icon::CHECK, 12.5, fg);
    }
    widgets::text_at(ui, pos2(rect.left() + 25.0, rect.center().y - 8.5), label, theme::regular(13.0), fg, rect.width() - 32.0);
    resp
}

/// Small heading above a settings card, with an optional value on the right.
pub(super) fn section_label(ui: &mut Ui, title: &str, value: Option<&str>) {
    ui.add_space(30.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    ui.painter().text(pos2(rect.left() + 2.0, rect.center().y), Align2::LEFT_CENTER, title, theme::bold(20.0), theme::text());
    if let Some(v) = value {
        ui.painter().text(pos2(rect.right() - 2.0, rect.center().y), Align2::RIGHT_CENTER, v, theme::semibold(13.0), theme::text_dim());
    }
    ui.add_space(10.0);
}

/// Whether a folding part of Settings is open (remembered; closed at first).
fn fold_state(ui: &Ui, key: &str) -> (egui::Id, bool) {
    let id = egui::Id::unique(("settings-fold", key));
    (id, ui.data_mut(|d| *d.get_persisted_mut_or_default::<bool>(id)))
}

/// A chevron that turns as `open` animates.
fn chevron(ui: &Ui, at: egui::Pos2, id: egui::Id, open: bool, color: Color32) {
    let k = theme::anim_bool(ui.ctx(), id.with("turn"), open, 0.2);
    // Drawn, not the font's two glyphs: it turns through the quarter between them.
    let (sin, cos) = (k * std::f32::consts::FRAC_PI_2).sin_cos();
    let turn = |x: f32, y: f32| at + vec2(x * cos - y * sin, x * sin + y * cos);
    let points = vec![turn(-2.0, -4.5), turn(2.5, 0.0), turn(-2.0, 4.5)];
    ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.7, color)));
}

/// A fold's body, opening and closing over a fifth of a second rather than all at once:
/// laid out at its full height and clipped to the part that's open, fading as it goes.
/// The height is the one measured the frame before.
fn fold_body(ui: &mut Ui, id: egui::Id, open: bool, body: impl FnOnce(&mut Ui)) {
    let t = theme::anim_bool(ui.ctx(), id.with("body"), open, 0.22);
    if t <= 0.0 {
        return;
    }
    let height_id = id.with("body-h");
    let full: f32 = ui.data(|d| d.get_temp(height_id)).unwrap_or(0.0);
    let at = ui.cursor().min;
    let w = ui.available_width();
    let shown = Rect::from_min_size(at, vec2(w, if t >= 1.0 { full } else { full * t }));
    let mut child = ui.new_child(egui::UiBuilder::new().id_salt(height_id).max_rect(Rect::from_min_size(at, vec2(w, full.max(1.0)))));
    if t < 1.0 {
        child.set_clip_rect(shown.intersect(ui.clip_rect()));
        child.multiply_opacity(t);
        super::animate(ui.ctx());
    }
    body(&mut child);
    let measured = child.min_rect().height();
    if (measured - full).abs() > 0.5 {
        // First shown, or its content changed: the room it takes follows next frame.
        ui.data_mut(|d| d.insert_temp(height_id, measured));
        ui.ctx().request_repaint();
    }
    ui.allocate_rect(Rect::from_min_size(at, vec2(w, if t >= 1.0 { measured } else { full * t })), Sense::hover());
}

/// A section heading that folds its section away; a click toggles it. True when open.
fn section_fold(ui: &mut Ui, title: &str) -> bool {
    let (id, mut open) = fold_state(ui, title);
    ui.add_space(30.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let resp = named(resp, title).on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(id, open));
    }
    let h = if resp.hovered() { 1.0 } else { 0.0 };
    ui.painter().text(pos2(rect.left() + 2.0, rect.center().y), Align2::LEFT_CENTER, title, theme::bold(20.0), theme::text());
    chevron(ui, pos2(rect.right() - 10.0, rect.center().y), id, open, theme::lerp_color(theme::text_dim(), theme::text(), h));
    ui.add_space(10.0);
    open
}

pub fn settings_card(ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(2, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
}

/// A settings row: title and description on the left, the control on the right.
pub(super) fn setting_row(ui: &mut Ui, title: &str, desc: Option<&str>, control: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    let text_w = (w - 230.0).max(200.0);
    let title_g = widgets::galley(ui, title, theme::semibold(15.5), theme::text(), text_w);
    let desc_g = desc.map(|d| widgets::galley_wrapped(ui, d, theme::regular(11.0), theme::text_dim(), text_w, 3));
    let text_h = title_g.size().y + desc_g.as_ref().map_or(0.0, |g| g.size().y + 3.0);
    let h = (text_h + 26.0).max(58.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let fade = if ui.is_enabled() { 1.0 } else { 0.45 };
    let top = rect.center().y - text_h / 2.0;
    ui.painter().galley(pos2(rect.left(), top), title_g.clone(), theme::with_alpha(theme::text(), fade));
    if let Some(g) = desc_g {
        ui.painter().galley(pos2(rect.left(), top + title_g.size().y + 3.0), g, theme::with_alpha(theme::text_dim(), fade));
    }
    let ctl = Rect::from_min_max(pos2(rect.left() + text_w + 20.0, rect.top()), rect.max);
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(ctl).layout(egui::Layout::right_to_left(egui::Align::Center)));
    control(&mut cui);
}

pub(super) fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::glass(0.07));
}

/// On/off switch tinted with the accent.
pub(super) fn toggle(ui: &mut Ui, on: &mut bool, name: &str) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(46.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let value = *on;
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), value, name));
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let t = theme::anim_bool(ui.ctx(), resp.id.with("t"), value, 0.16);
    let h = if resp.hovered() { 1.0 } else { 0.0 };
    let accent = ui.ctx().data(|d| d.get_temp::<Color32>(egui::Id::unique("accent"))).unwrap_or(theme::RED);
    let fade = if ui.is_enabled() { 1.0 } else { 0.4 };
    let off = theme::glass(0.16 + 0.06 * h);
    ui.painter().rect_filled(rect, 11.0, theme::with_alpha(theme::lerp_color(off, accent, t), fade));
    let knob = pos2(egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t), rect.center().y);
    ui.painter().circle_filled(knob + vec2(0.0, 1.0), 8.0, theme::shade(0.25 * fade));
    ui.painter().circle_filled(knob, 8.0, theme::with_alpha(Color32::WHITE, fade));
    resp
}

/// Whole seconds on a track like the volume slider's, with the ends labelled and the
/// value beside it: click or drag to set (scrolling over it scrolls the page).
fn seconds_slider(ui: &mut Ui, v: &mut u32, range: std::ops::RangeInclusive<u32>, name: &str) -> egui::Response {
    let (lo, hi) = (*range.start(), *range.end());
    let label = |ui: &mut Ui, text: String, w: f32, font: egui::FontId, color: Color32| {
        let (rect, _) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
        let g = widgets::galley(ui, &text, font, color, w);
        ui.painter().galley(pos2(rect.right() - g.size().x, rect.center().y - g.size().y / 2.0), g, color);
    };
    label(ui, format!("{v} s"), 34.0, theme::semibold(13.0), theme::text());
    ui.add_space(6.0);
    label(ui, format!("{hi} s"), 26.0, theme::regular(12.0), theme::text_dim());
    let (rect, mut resp) = ui.allocate_exact_size(vec2(170.0, 26.0), Sense::click_and_drag());
    label(ui, format!("{lo} s"), 22.0, theme::regular(12.0), theme::text_dim());
    let track = Rect::from_center_size(rect.center(), vec2(rect.width() - 16.0, 4.0));
    let old = *v;
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.dragged() || resp.clicked())
    {
        let t = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
        *v = (lo as f32 + t * (hi - lo) as f32).round() as u32;
    }
    if *v != old {
        resp.mark_changed();
    }
    let value = *v;
    resp.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), value as f64, name));
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let h = if resp.hovered() || resp.dragged() { 1.0 } else { 0.0 };
    let accent = ui.ctx().data(|d| d.get_temp::<Color32>(egui::Id::unique("accent"))).unwrap_or(theme::RED);
    let t = (value - lo) as f32 / (hi - lo) as f32;
    let knob = pos2(egui::lerp(track.left()..=track.right(), t), track.center().y);
    ui.painter().rect_filled(track, 2.0, theme::glass(0.15));
    ui.painter().rect_filled(Rect::from_min_max(track.min, pos2(knob.x, track.max.y)), 2.0, accent);
    ui.painter().circle_filled(knob + vec2(0.0, 1.0), 7.0 + h, theme::shade(0.25));
    ui.painter().circle_filled(knob, 7.0 + h, Color32::WHITE);
    resp
}

/// Draw a key cap whose right edge is at `right`; returns its left edge.
fn keycap(ui: &Ui, right: egui::Pos2, key: &str) -> f32 {
    let g = widgets::galley(ui, key, theme::semibold(12.5), theme::text(), 200.0);
    let size = vec2((g.size().x + 18.0).max(30.0), 26.0);
    let rect = Rect::from_min_size(pos2(right.x - size.x, right.y - size.y / 2.0), size);
    ui.painter().rect_filled(rect, 7.0, theme::glass(0.08));
    ui.painter().rect_stroke(rect, 7.0, egui::Stroke::new(1.0, theme::glass(0.12)), egui::StrokeKind::Inside);
    ui.painter().rect_filled(Rect::from_min_max(pos2(rect.left() + 2.0, rect.bottom() - 2.0), pos2(rect.right() - 2.0, rect.bottom())), 2.0, theme::shade(0.25));
    ui.painter().galley(rect.center() - g.size() / 2.0, g, theme::text());
    rect.left()
}


#[cfg(test)]
mod tests {
    use crate::backend::Collection;

    #[test]
    fn played_artists_are_newest_first_and_once() {
        use crate::model::{Link, Track};
        let row = |artists: Vec<(&str, Option<&str>)>| Track {
            id: String::new(),
            title: String::new(),
            artists: artists.into_iter().map(|(n, id)| Link { name: n.into(), id: id.map(str::to_owned) }).collect(),
            album: None,
            duration: None,
            thumbs: vec![],
            track_nr: None,
            plays: None,
            music_video: false,
        };
        let history = vec![row(vec![("A", Some("id-a")), ("Unlinked", None)]), row(vec![("B", Some("id-b")), ("A", Some("id-a"))])];
        let names: Vec<_> = super::played_artists(&history).into_iter().map(|a| a.name).collect();
        assert_eq!(names, ["A", "B"]);
    }

    #[test]
    fn home_subtitles_lose_their_counts() {
        assert_eq!(super::without_counts("Song • The Weeknd • 1.2B plays"), "Song • The Weeknd");
        assert_eq!(super::without_counts("Playlist • YouTube Music • 30K views"), "Playlist • YouTube Music");
        assert_eq!(super::without_counts("Album • 2Pac"), "Album • 2Pac");
    }

    #[test]
    fn test_collection_genre_detection() {
        let make_coll = |title: &str, subtitle: &str, desc: Option<&str>, is_album: bool| Collection {
            id: "test_coll".into(),
            is_album,
            save_id: None,
            title: title.into(),
            subtitle: subtitle.into(),
            artists: Vec::new(),
            description: desc.map(str::to_owned),
            thumbs: Vec::new(),
            tracks: Vec::new(),
            set_ids: Vec::new(),
            count: None,
            owned: false,
        };

        // Chromeo Adult Contemporary -> ELECTRONIC
        let c1 = make_coll("Adult Contemporary", "Chromeo • 2024", Some("Electro-funk duo Chromeo"), true);
        assert_eq!(super::collection_genre(&c1), "ELECTRONIC");

        // Hip-hop
        let c2 = make_coll("Good Kid, M.A.A.D City", "Kendrick Lamar • 2012", Some("West coast rap and hip hop"), true);
        assert_eq!(super::collection_genre(&c2), "HIP-HOP");

        // Rock
        let c3 = make_coll("Nevermind", "Nirvana • 1991", Some("Grunge rock album"), true);
        assert_eq!(super::collection_genre(&c3), "ROCK");

        // R&B
        let c4 = make_coll("Blonde", "Frank Ocean • 2016", Some("Contemporary R&B and soul"), true);
        assert_eq!(super::collection_genre(&c4), "R&B");

        // Pop
        let c5 = make_coll("1989", "Taylor Swift • 2014", Some("Synth pop album"), true);
        assert_eq!(super::collection_genre(&c5), "ELECTRONIC"); // "synth" keyword -> ELECTRONIC

        let c5b = make_coll("Teenage Dream", "Katy Perry • 2010", Some("Mainstream pop record"), true);
        assert_eq!(super::collection_genre(&c5b), "POP");

        // Default album
        let c6 = make_coll("Unknown Album", "2024", None, true);
        assert_eq!(super::collection_genre(&c6), "ELECTRONIC");

        // Playlist
        let c7 = make_coll("Chill Mix", "Various Artists", None, false);
        assert_eq!(super::collection_genre(&c7), "PLAYLIST");
    }

    #[test]
    fn test_peach_color_constants() {
        use egui::Color32;
        assert_eq!(super::PEACH, Color32::from_rgb(0xE0, 0x7A, 0x5F));
        assert_eq!(super::PEACH_TEXT, Color32::from_rgb(0x1a, 0x14, 0x12));
    }
}
