//! App chrome: background, sidebar, top bar, player bar, queue drawer, toasts.

use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use eframe::egui_glow::glow::{self, HasContext};
use egui::{Align2, Color32, Pos2, Rect, Sense, Ui, Vec2, pos2, vec2};

use super::widgets::{self, icon_at, icon_button, named, paint_cover, text_at};
use super::{Action, App, LibTab, NpTab, Page, Repeat, fmt_time, glyphs};
use crate::i18n::t;
use crate::model::Target;
use crate::shader::Params;
use crate::sync::MutexExt;
use crate::theme::{self, icon};

// ------------------------------------------------------------------ background

/// Vertical gradient (top colour → bottom colour) over `rect`.
pub fn vgradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    gradient(painter, rect, [top, top, bottom, bottom]);
}

/// Vertical gradient over a rounded rectangle, fading smoothly with ease-in.
#[allow(dead_code)]
pub fn rounded_vgradient(painter: &egui::Painter, rect: Rect, radius: f32, top: Color32, bottom: Color32) {
    if radius <= 0.0 {
        vgradient(painter, rect, top, bottom);
        return;
    }
    let r = radius.min(rect.width() * 0.5).min(rect.height() * 0.5);
    let mut mesh = egui::Mesh::default();
    let center = rect.center();
    let c_mid = {
        let t = 0.5_f32;
        theme::lerp_color(top, bottom, t * t)
    };
    mesh.colored_vertex(center, c_mid);

    let mut add_arc = |cx: f32, cy: f32, start_a: f32, end_a: f32| {
        const STEPS: usize = 6;
        for i in 0..=STEPS {
            let a = start_a + (end_a - start_a) * (i as f32 / STEPS as f32);
            let (s, c) = a.sin_cos();
            let p = pos2(cx + c * r, cy + s * r);
            let t = ((p.y - (rect.top() + rect.height() * 0.15)) / (rect.height() * 0.85)).clamp(0.0, 1.0);
            mesh.colored_vertex(p, theme::lerp_color(top, bottom, t * t));
        }
    };

    use std::f32::consts::PI;
    add_arc(rect.right() - r, rect.top() + r, -PI * 0.5, 0.0);
    add_arc(rect.right() - r, rect.bottom() - r, 0.0, PI * 0.5);
    add_arc(rect.left() + r, rect.bottom() - r, PI * 0.5, PI);
    add_arc(rect.left() + r, rect.top() + r, PI, PI * 1.5);

    let n = mesh.vertices.len() as u32;
    for i in 1..n {
        let next = if i + 1 < n { i + 1 } else { 1 };
        mesh.add_triangle(0, i, next);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Horizontal gradient (left colour → right colour) over `rect`.
#[allow(dead_code)]
pub fn hgradient(painter: &egui::Painter, rect: Rect, left: Color32, right: Color32) {
    gradient(painter, rect, [left, right, left, right]);
}

/// Corner colours: left top, right top, left bottom, right bottom.
fn gradient(painter: &egui::Painter, rect: Rect, c: [Color32; 4]) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), c[0]);
    mesh.colored_vertex(rect.right_top(), c[1]);
    mesh.colored_vertex(rect.left_bottom(), c[2]);
    mesh.colored_vertex(rect.right_bottom(), c[3]);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(egui::Shape::mesh(mesh));
}

/// The background mode browse pages use: the theme's own choice, else whatever now
/// playing uses.
fn page_mode(look: &crate::preset::Preset) -> crate::preset::BgMode {
    use crate::preset::{BgMode, PageBg};
    match look.app.page_bg {
        PageBg::Same => look.background.mode,
        PageBg::Static => BgMode::CoverStatic,
        PageBg::Accent => BgMode::AccentGradient,
        PageBg::Solid => BgMode::Solid,
    }
}

/// Whether the pages' background moves while music plays (static themes need no frames).
pub fn pages_bg_moves(look: &crate::preset::Preset) -> bool {
    use crate::preset::BgMode;
    let b = &look.background;
    matches!(page_mode(look), BgMode::Kawarp | BgMode::CoverFullbleed) && b.speed > 0.0 && b.intensity > 0.0
}

/// `only`: paint just this part (the rest is covered by now playing).
/// The background's flat colour, under its art and gradients.
pub fn base_color(app: &App) -> Color32 {
    app.page_target_color()
}

pub fn background(app: &mut App, ui: &mut Ui, screen: Rect, now: f64, only: Option<Rect>) {
    use crate::preset::BgMode;
    let dt = (now - app.last_frame).clamp(0.0, 0.1);
    app.last_frame = now;
    let look = app.look();
    let b = &look.background;
    let mode = page_mode(look);
    // Light pages are the art at full strength under a white wash, a pale tint of the cover.
    let light = theme::light();
    let dim = if light { 1.0 } else { look.app.page_dim.unwrap_or(b.page_dim).clamp(0.02, 1.0) };
    // Now playing animates the shader itself, so keep the clock running for it too.
    let animated = matches!(b.mode, BgMode::Kawarp | BgMode::CoverFullbleed) && b.speed > 0.0;
    let moving = animated && app.playing();
    if moving {
        app.bg_time += dt * b.speed as f64;
    }
    let painter = match only {
        Some(r) => ui.painter().with_clip_rect(r),
        None => ui.painter().clone(),
    };
    painter.rect_filled(screen, 0.0, app.page_color(ui.ctx()));
    let art = matches!(mode, BgMode::Kawarp | BgMode::CoverStatic | BgMode::CoverFullbleed) && !matches!(app.page, Page::Home);
    if let Some(bg) = app.bg.as_ref().filter(|_| art) {
        let blend = theme::ease_in_out(((now - app.art_changed) / 1.4) as f32);
        if blend < 1.0 {
            // The crossfade to new art finishes even while paused.
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        }
        let level = if app.reactive_bg && app.playing() { app.audio.status.level() * b.reactive } else { 0.0 };
        let still = mode == BgMode::CoverStatic;
        bg.paint(
            &painter,
            screen,
            Params {
                time: app.bg_time as f32,
                blend,
                intensity: if still { 0.0 } else { b.intensity * 0.9 },
                saturation: b.saturation * 0.9,
                dim,
                scale: (b.scale - 0.08).max(1.0) + if still { 0.0 } else { level.min(0.5) * 0.08 },
            },
        );
    } else if mode == BgMode::Solid || look.id == crate::preset::DEFAULT_ID {
        // Plain colour or custom ambient page colour: nothing over it.
    } else {
        painter.rect_filled(screen, 0.0, look.solid());
    }
    if light && (art || look.id != crate::preset::DEFAULT_ID) {
        painter.rect_filled(screen, 0.0, Color32::from_white_alpha(212));
    }
}

// ------------------------------------------------------------------ sidebar

/// How the theme draws nav items.
#[derive(Clone, Copy)]
struct NavLook {
    style: crate::preset::NavStyle,
    radius: f32,
    accent: Color32,
}

impl NavLook {
    fn of(app: &App) -> Self {
        Self { style: app.look().app.nav, radius: (app.look().chrome.radius * 0.6).clamp(4.0, 16.0), accent: app.accent_color() }
    }
}

/// A nav item's background in the theme's nav style; returns its icon colour.
/// `h` and `s` are the hover and selected animations (0..1).
fn nav_bg(ui: &Ui, look: NavLook, rect: Rect, h: f32, s: f32) -> Color32 {
    use crate::preset::NavStyle;
    let (nav_r, accent) = (look.radius, look.accent);
    let icon_fg = theme::lerp_color(theme::text_dim(), theme::text(), (h + s).min(1.0));
    match look.style {
        NavStyle::Pill => {
            ui.painter().rect_filled(rect, nav_r, theme::glass(0.05 * h + 0.10 * s));
            icon_fg
        }
        NavStyle::Accent => {
            ui.painter().rect_filled(rect, nav_r, theme::glass(0.05 * h + 0.08 * s));
            accent
        }
        NavStyle::Bar => {
            ui.painter().rect_filled(rect, nav_r, theme::glass(0.05 * h));
            let bar = Rect::from_center_size(pos2(rect.left() + 2.0, rect.center().y), vec2(3.0, 20.0 * s));
            ui.painter().rect_filled(bar, 2.0, accent);
            icon_fg
        }
    }
}

/// Sidebar width (points).
pub const SIDEBAR_W: f32 = 220.0;

/// A button on the sidebar's top line, at `rect`: a rounded rectangle under the
/// pointer, with `paint` drawing its mark at the centre in the given colour.
fn top_button(ui: &mut Ui, rect: Rect, key: &str, name: &str, paint: impl FnOnce(&Ui, Pos2, Color32)) -> bool {
    let resp = ui.interact(rect, ui.scope_id().with(("side-top", key)), Sense::click());
    let resp = named(resp, name).on_hover_text(name).on_hover_cursor(egui::CursorIcon::PointingHand);
    let h = if resp.hovered() { 1.0 } else { 0.0 };
    if h > 0.0 {
        ui.painter().rect_filled(rect, 6.0, theme::glass(0.10 * h));
    }
    paint(ui, rect.center(), theme::lerp_color(theme::text_dim(), theme::text(), h));
    resp.clicked()
}

/// The chevron pair that walks the page history.
fn history_button(ui: &mut Ui, rect: Rect, glyph: char, name: &str) -> bool {
    top_button(ui, rect, name, name, |ui, at, c| icon_at(ui, at, glyph, 16.0, c))
}

/// The sidebar: the search box, the app's pages and the playlists.
pub fn sidebar(app: &mut App, ui: &mut Ui) {
    const ICON_X: f32 = 24.0;
    // Content width: text keeps its truncation inside it.
    let full_w = ui.available_width();

    // The page history buttons, on the macOS window buttons' line and right after them
    // (the buttons' centres are 16 points down, the last one ending some 62 in). A
    // floating sidebar starts beside the window buttons, not under: it keeps the row.
    let top = ui.cursor().min;
    let screen = ui.ctx().content_rect();
    let below = pos2(top.x + ICON_X - 14.0, top.y + 3.0);
    let on_line = top.y - screen.top() >= 36.0;
    let at = if on_line { pos2(screen.left() + 76.0, screen.top() + 3.0) } else { below };
    let button = |i: usize| Rect::from_min_size(at + vec2([0.0, 30.0][i], 0.0), vec2(28.0, 26.0));
    if history_button(ui, button(0), icon::CHEVRON_LEFT, "Back") {
        app.nav_back();
    }
    if history_button(ui, button(1), icon::CHEVRON_RIGHT, "Forward") {
        app.nav_forward();
    }
    // The row's own room: all of it under the window buttons, none beside them.
    ui.add_space(if on_line { 4.0 } else { 46.0 });

    let mut go_to: Option<Page> = None;
    let look = NavLook::of(app);
    // Draws one row, with the measures of Music's sidebar (30 points, 13-point labels):
    // `paint` puts its icon at the given point in the given colour.
    let row = |ui: &mut Ui, paint: &dyn Fn(&Ui, Pos2, Color32), label: &str, page: Page, selected: bool, go_to: &mut Option<Page>| {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
        let resp = named(resp, label).on_hover_cursor(egui::CursorIcon::PointingHand);
        let h = if resp.hovered() { 1.0 } else { 0.0 };
        let s = theme::anim_bool(ui.ctx(), resp.id.with("s"), selected, 0.2);
        let icon_fg = nav_bg(ui, look, rect, h, s);
        paint(ui, pos2(rect.left() + ICON_X, rect.center().y), icon_fg);
        let fg = theme::label();
        let font = if selected { theme::semibold(13.0) } else { theme::regular(13.0) };
        ui.painter().text(pos2(rect.left() + 44.0, rect.center().y), Align2::LEFT_CENTER, label, font, fg);
        if resp.clicked() {
            *go_to = Some(page);
        }
    };
    let icon_row = |ui: &mut Ui, glyph: char, label: &str, page: Page, selected: bool, go_to: &mut Option<Page>| {
        row(ui, &|ui: &Ui, at: Pos2, c: Color32| icon_at(ui, at, glyph, 15.0, c), label, page, selected, go_to);
    };
    // A section's heading; a click folds the section away (remembered across launches).
    let section = |ui: &mut Ui, title: &str| -> bool {
        let key = egui::Id::unique(("sidebar-section", title));
        let mut open = ui.data_mut(|d| *d.get_persisted_mut_or_insert_with(key, || true));
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
        let resp = named(resp, title).on_hover_cursor(egui::CursorIcon::PointingHand);
        let h = if resp.hovered() { 1.0 } else { 0.0 };
        let c = theme::lerp_color(theme::text_faint(), theme::text_dim(), h);
        ui.painter().text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, title, theme::semibold(11.0), c);
        let k = theme::anim_bool(ui.ctx(), resp.id.with("open"), open, 0.15);
        icon_at(ui, pos2(r.right() - 12.0, r.center().y), if k > 0.5 { icon::CHEVRON_DOWN } else { icon::CHEVRON_RIGHT }, 12.0, c);
        if resp.clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_persisted(key, open));
        }
        open
    };
    let nav = |ui: &mut Ui, glyph: &'static [u8], size: f32, label: &str, page: Page, selected: bool, go_to: &mut Option<Page>| {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
        let resp = named(resp, label).on_hover_cursor(egui::CursorIcon::PointingHand);
        let h = if resp.hovered() { 1.0 } else { 0.0 };
        let s = theme::anim_bool(ui.ctx(), resp.id.with("s"), selected, 0.2);
        let icon_fg = nav_bg(ui, look, rect, h, s);
        glyphs::draw(ui, pos2(rect.left() + ICON_X, rect.center().y), size, glyph, icon_fg);
        let fg = theme::label();
        let font = if selected { theme::semibold(13.0) } else { theme::regular(13.0) };
        ui.painter().text(pos2(rect.left() + 44.0, rect.center().y), Align2::LEFT_CENTER, label, font, fg);
        if resp.clicked() {
            *go_to = Some(page);
        }
    };
    let p = app.page.clone();
    // A size down from the desktop app's, for the shorter rows.
    const APPLE: f32 = glyphs::APPLE_GLYPH - 3.0;
    const LUCIDE: f32 = glyphs::LUCIDE_GLYPH - 2.0;
    search_field(app, ui, ui.available_width());
    ui.add_space(6.0);

    let query_trim = app.search_text.trim();
    let has_search_drop = app.search_drop.as_deref().is_some_and(|q| !q.trim().is_empty());
    let has_query = !query_trim.is_empty();
    let show_suggestions = (app.search_focused || app.forget_held) && !app.suggestions.1.is_empty();
    let search_active = has_search_drop || has_query || show_suggestions;

    // Drag-to-reorder and right-click-to-hide results, applied once the rows are done.
    let mut reorder: Option<(String, String)> = None;
    let mut hide: Option<String> = None;

    let transferring = super::transfer::sidebar_visible(app, ui.input(|i| i.time));
    let ready = app.updater.as_ref().and_then(|u| match u.state() {
        crate::update::State::Ready { version, .. } => Some(version),
        _ => None,
    });
    // Room under the list: Settings, the app's sign-off, and the transfer card.
    let bottom_h = (if transferring { 106.0 } else { 42.0 }) + if ready.is_some() { 58.0 } else { 0.0 };

    if search_active {
        let list_h = (ui.available_height() - bottom_h).max(0.0);
        ui.allocate_ui(vec2(ui.available_width(), list_h), |ui| {
            sidebar_search_content(app, ui);
        });
    } else {
        if section(ui, if app.spotify_mode { "Spotify" } else { "YouTube Music" }) {
            nav(ui, glyphs::HOME, APPLE, &t!("home"), Page::Home, p == Page::Home, &mut go_to);
            nav(ui, glyphs::EXPLORE, APPLE, &t!("new"), Page::New, p == Page::New, &mut go_to);
        }
        ui.add_space(4.0);
        if section(ui, &t!("library")) {
            let lib = |tab: LibTab| p == Page::Library(tab);
            nav(ui, glyphs::LIBRARY, APPLE, &t!("playlists"), Page::Library(LibTab::Playlists), lib(LibTab::Playlists), &mut go_to);
            icon_row(ui, icon::CIRCLE_USER, &t!("artists"), Page::Library(LibTab::Artists), lib(LibTab::Artists), &mut go_to);
            icon_row(ui, icon::DISC_3, &t!("albums"), Page::Library(LibTab::Albums), lib(LibTab::Albums), &mut go_to);
            icon_row(ui, icon::HEART, &t!("liked_songs"), Page::Library(LibTab::Songs), lib(LibTab::Songs), &mut go_to);
            if !app.spotify_mode {
                icon_row(ui, icon::HISTORY, &t!("recently_played"), Page::Library(LibTab::History), lib(LibTab::History), &mut go_to);
            }
            nav(ui, glyphs::DOWNLOAD, APPLE, &t!("downloads"), Page::Downloads, p == Page::Downloads, &mut go_to);
        }
        ui.add_space(4.0);
        let playlists_open = section(ui, &t!("playlists"));

        // Playlists that appeared since the order was last changed join it at the end, so a
        // drag can place any of them.
        if let Some(l) = if app.spotify_mode { &app.spotify_library } else { &app.library } {
            for c in &l.playlists {
                if let Target::Playlist(id) = &c.target
                    && !app.playlist_order.contains(id)
                {
                    app.playlist_order.push(id.clone());
                }
            }
        }

        let pl_h = (ui.available_height() - bottom_h).max(0.0);
        ui.allocate_ui(vec2(ui.available_width(), pl_h), |ui| {
            if !app.active_logged_in() {
                if app.spotify_mode {
                    ui.label(egui::RichText::new("Sign in to Spotify to see your playlists.").color(theme::text_dim()));
                    ui.add_space(8.0);
                    if widgets::pill(ui, "Open Settings", None, false).clicked() {
                        go_to = Some(Page::Settings);
                    }
                } else {
                    ui.label(egui::RichText::new(t!("sign_in_to_see_playlists")).color(theme::text_dim()));
                    ui.add_space(8.0);
                    if widgets::pill(ui, &t!("sign_in"), None, false).clicked() {
                        app.start_sign_in(ui.ctx());
                    }
                }
                return;
            }
            if !playlists_open {
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("sidebar-pl")
                .auto_shrink([false, false])
                .max_height(pl_h)
                .show(ui, |ui| {
                // Last launch's list until the library arrives.
                let fallback_cache = if app.spotify_mode { None } else { app.sidebar_cache.as_ref() };
                let Some(playlists) = (if app.spotify_mode { app.spotify_library.as_ref() } else { app.library.as_ref() }).map(|l| &l.playlists).or(fallback_cache).cloned() else {
                    // The spinner animates every frame: give up on a request that never answered.
                    if app.library_requested.elapsed() < std::time::Duration::from_secs(20) {
                        ui.horizontal(|ui| {
                            ui.add_space(ICON_X - 8.0);
                            ui.spinner();
                        });
                    } else {
                        ui.label(egui::RichText::new(t!("could_not_load_playlists")).color(theme::text_dim()));
                        ui.add_space(8.0);
                        if widgets::pill(ui, &t!("try_again"), None, false).clicked() {
                            app.request_library();
                        }
                    }
                    return;
                };
                let mut visible: Vec<&crate::model::Card> = playlists
                    .iter()
                    .filter(|c| !matches!(&c.target, Target::Playlist(id) if app.hidden_playlists.contains(id)))
                    .collect();
                visible.sort_by_key(|c| match &c.target {
                    Target::Playlist(id) => app.playlist_order.iter().position(|x| x == id).unwrap_or(usize::MAX),
                    _ => usize::MAX,
                });
                let drag_key = egui::Id::unique("sidebar-pl-drag");
                let dragging: Option<String> = ui.data(|d| d.get_temp(drag_key));
                let down = ui.input(|i| i.pointer.any_down());
                let pointer = ui.input(|i| i.pointer.interact_pos());
                for c in &visible {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click_and_drag());
                    let id = match &c.target {
                        Target::Playlist(id) => id.clone(),
                        _ => continue,
                    };
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let is_dragged = dragging.as_deref() == Some(id.as_str());
                    let resp = named(resp, &c.title).on_hover_cursor(if is_dragged { egui::CursorIcon::Grabbing } else { egui::CursorIcon::PointingHand });
                    if resp.drag_started() {
                        ui.data_mut(|d| d.insert_temp(drag_key, id.clone()));
                    }
                    if let Some(did) = &dragging
                        && did != &id
                        && down
                        && pointer.is_some_and(|p| rect.contains(p))
                    {
                        reorder = Some((did.clone(), id.clone()));
                    }
                    let h = if resp.hovered() { 1.0 } else { 0.0 };
                    let selected = matches!(&c.target, Target::Playlist(id) if app.page == Page::Playlist(id.clone()));
                    let lift = if is_dragged { 0.10 } else { 0.0 };
                    ui.painter().rect_filled(rect, look.radius, theme::glass(0.05 * h + if selected { 0.08 } else { 0.0 } + lift));
                    let is_liked = id == super::LIKED_MUSIC
                        || id == "LM"
                        || id == "VLLM"
                        || id == crate::spotify::SPOTIFY_LIKED_ID
                        || c.thumbs.iter().any(|t| t.url.contains("/images/pbg/liked-songs"));
                    if is_liked {
                        let icon_fg = if selected {
                            look.accent
                        } else {
                            theme::lerp_color(theme::text_dim(), theme::text(), h)
                        };
                        icon_at(ui, pos2(rect.left() + ICON_X, rect.center().y), icon::HEART, 15.0, icon_fg);
                    } else {
                        let img = Rect::from_center_size(pos2(rect.left() + ICON_X, rect.center().y), Vec2::splat(22.0));
                        paint_cover(ui, img, &c.thumbs, theme::thumb_radius(), 1.0);
                    }
                    let w = full_w - 54.0;
                    let fg = theme::label();
                    text_at(ui, pos2(rect.left() + 44.0, rect.center().y - 8.5), &c.title, theme::regular(13.0), fg, w);
                    egui::Popup::context_menu(&resp).show(|ui| {
                        ui.set_min_width(170.0);
                        if widgets::menu_action(ui, None, &t!("hide_from_sidebar")) {
                            hide = Some(id.clone());
                        }
                    });
                    if resp.clicked() && !is_dragged {
                        app.actions.push(Action::Open(c.target.clone()));
                    }
                }
                if ui.input(|i| i.pointer.any_released()) {
                    ui.data_mut(|d| d.remove_temp::<String>(drag_key));
                }
            });
        });
    }
    if let Some(id) = hide {
        app.hidden_playlists.insert(id);
    }
    if let Some((dragged, target)) = reorder
        && let (Some(from), Some(to)) = (app.playlist_order.iter().position(|x| x == &dragged), app.playlist_order.iter().position(|x| x == &target))
    {
        let id = app.playlist_order.remove(from);
        app.playlist_order.insert(to.min(app.playlist_order.len()), id);
    }
    let mut restart = false;
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        nav(ui, glyphs::SETTINGS, LUCIDE, &t!("settings"), Page::Settings, p == Page::Settings, &mut go_to);
        if transferring {
            super::transfer::sidebar_card(app, ui, ICON_X);
        }
        if let Some(v) = &ready {
            ui.add_space(6.0);
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
            let resp = named(resp, &t!("restart_to_update")).on_hover_cursor(egui::CursorIcon::PointingHand);
            let h = if resp.hovered() { 1.0 } else { 0.0 };
            ui.painter().rect_filled(rect, 12.0, theme::with_alpha(look.accent, 0.22 + 0.12 * h));
            glyphs::draw(ui, pos2(rect.left() + ICON_X, rect.center().y), LUCIDE, glyphs::SPARKLES, theme::text());
            ui.painter().text(pos2(rect.left() + 46.0, rect.top() + 17.0), Align2::LEFT_CENTER, t!("restart_to_update"), theme::semibold(13.5), theme::text());
            ui.painter().text(pos2(rect.left() + 46.0, rect.top() + 35.0), Align2::LEFT_CENTER, format!("Dawn FM {v}"), theme::regular(12.0), theme::with_alpha(theme::text(), 0.75));
            restart = resp.clicked();
        }
    });
    if restart {
        app.restart_to_update(ui.ctx());
    }
    if let Some(page) = go_to {
        if matches!(page, Page::Search(_)) {
            // Search lands on the search page with the caret in the box; already there,
            // it just brings the box back.
            app.search_refocus = true;
            if !matches!(app.page, Page::Search(_)) {
                app.actions.push(Action::Go(Page::Search(String::new())));
            }
        } else {
            app.actions.push(Action::Go(page));
        }
    }
}

/// The search box: typeahead, suggestions and Enter to search. The Search page draws
/// it; `app.search_rect` anchors the suggestions under it.
pub fn search_field(app: &mut App, ui: &mut Ui, width: f32) {
    // Anywhere on it focuses the box, not just on the text area inside it.
    let (rect, field) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    let field = field.on_hover_cursor(egui::CursorIcon::Text);
    let focus = theme::anim_bool(ui.ctx(), "search-focus", app.search_focused, 0.2);
    let radius = 10.0;
    if theme::light() {
        ui.painter().rect_filled(rect, radius, theme::glass(0.08));
        if focus > 0.0 {
            let stroke_color = theme::with_alpha(app.accent_color(), focus);
            ui.painter().rect_stroke(rect, radius, egui::Stroke::new(1.5, stroke_color), egui::StrokeKind::Inside);
        }
    } else {
        ui.painter().rect_filled(rect, radius, Color32::from_rgba_unmultiplied(255, 255, 255, 18));
        if focus > 0.0 {
            let stroke_color = theme::with_alpha(app.accent_color(), focus);
            ui.painter().rect_stroke(rect, radius, egui::Stroke::new(1.5, stroke_color), egui::StrokeKind::Inside);
        }
    }
    icon_at(ui, pos2(rect.left() + 17.0, rect.center().y), icon::SEARCH, 15.0, theme::text_dim());
    let clearable = !app.search_text.is_empty();
    let inner = Rect::from_min_max(pos2(rect.left() + 34.0, rect.top() + 3.0), pos2(rect.right() - if clearable { 34.0 } else { 12.0 }, rect.bottom() - 3.0));

    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.search_text.clear();
        app.search_drop = None;
        app.suggestions = Default::default();
        app.suggest_sel = None;
        app.search_focused = false;
    }
    // Up and Down walk the suggestions (past either end back to what was typed). Taken
    // before the box sees them, where they'd move the caret. Only bare arrows (Shift
    // selects text), and only suggestions for what's typed now, not ones still being replaced.
    let fresh = app.suggestions.0 == app.search_text.trim();
    let n = if fresh { app.suggestions.1.len() } else { 0 };
    if app.search_focused && n > 0 && ui.input(|i| i.modifiers.is_none()) {
        ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                app.suggest_sel = match app.suggest_sel {
                    None => Some(0),
                    Some(s) if s + 1 < n => Some(s + 1),
                    Some(_) => None,
                };
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                app.suggest_sel = match app.suggest_sel {
                    None => Some(n - 1),
                    Some(0) => None,
                    Some(s) => Some(s - 1),
                };
            }
        });
    }
    let edit = ui.put(
        inner,
        egui::TextEdit::singleline(&mut app.search_text)
            .hint_text(egui::RichText::new(t!("search")).color(theme::text_faint()))
            .font(theme::regular(13.5))
            .frame(egui::Frame::NONE)
            .align(egui::Align2::LEFT_CENTER)
            .desired_width(inner.width()),
    );
    app.search_rect = Some(rect);
    if std::mem::take(&mut app.search_refocus) {
        edit.request_focus();
    }
    app.search_focused = edit.has_focus();
    let mut cleared = false;
    if clearable {
        let x = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(24.0));
        let resp = named(ui.interact(x, ui.scope_id().with("search-clear"), Sense::click()), "Clear search").on_hover_cursor(egui::CursorIcon::PointingHand);
        let h = if resp.hovered() { 1.0 } else { 0.0 };
        if h > 0.0 {
            ui.painter().circle_filled(x.center(), 12.0, theme::glass(0.10 * h));
        }
        icon_at(ui, x.center(), icon::X, 14.0, theme::lerp_color(theme::text_dim(), theme::text(), h));
        if resp.clicked() {
            // Back to the empty search page: the old results go with the text.
            app.search_text.clear();
            app.search_synced = String::new();
            app.suggestions = Default::default();
            app.suggest_sel = None;
            cleared = true;
            app.search_drop = None;
            edit.request_focus();
        }
    }
    if field.clicked() || ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
        edit.request_focus();
    }
    if edit.changed() || edit.gained_focus() || cleared {
        // Typing again swaps the results for suggestions.
        if edit.changed() || cleared {
            app.search_drop = None;
        }
        app.suggest_sel = None;
        let now = ui.input(|i| i.time);
        // Show cached results immediately, then refresh after a short typing pause.
        let first = app.suggestions.1.is_empty();
        let query = app.search_text.trim();
        app.suggestions = app.suggest_cache.iter()
            .find(|(q, _)| q == query)
            .cloned()
            .unwrap_or_default();
        let delay = if first || cleared || edit.gained_focus() { 0.0 } else { 0.06 };
        app.suggest_due = Some(now + delay);
    }
    // The event loop can otherwise sleep past the debounce deadline.
    if let Some(due) = app.suggest_due {
        let now = ui.input(|i| i.time);
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64((due - now).max(0.0)));
    }
    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        let picked = app.suggest_sel.filter(|_| fresh).and_then(|s| app.suggestions.1.get(s)).map(|s| s.text.clone());
        let q = picked.unwrap_or_else(|| app.search_text.trim().to_owned());
        if !q.is_empty() {
            app.suggestions = Default::default();
            app.search_drop = Some(q.clone());
            app.actions.push(Action::Go(Page::Search(q)));
        }
    }
}

/// Suggestions are now displayed directly within the sidebar list instead of an external floating menu.
pub fn suggestions(_app: &mut App, _ctx: &egui::Context) {}

/// Search results are now displayed directly within the sidebar list instead of an external floating menu.
pub fn search_results(_app: &mut App, _ctx: &egui::Context) {}

/// Shows search suggestions (typeahead / history) or search results directly in the sidebar list.
fn sidebar_search_content(app: &mut App, ui: &mut Ui) {
    let q_opt = app.search_drop.clone().or_else(|| {
        let t = app.search_text.trim();
        if !t.is_empty() && app.suggestions.1.is_empty() {
            Some(t.to_owned())
        } else {
            None
        }
    });

    if let Some(q) = q_opt {
        // Search Results inside sidebar
        egui::ScrollArea::vertical().id_salt("sidebar-search-results").auto_shrink(false).show(ui, |ui| {
            let Some(r) = app.search.as_ref().filter(|r| r.query == q) else {
                ui.add_space(36.0);
                ui.vertical_centered(|ui| {
                    ui.spinner();
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Searching…").font(theme::regular(13.0)).color(theme::text_dim()));
                });
                return;
            };

            if r.tracks.is_empty() && r.artists.is_empty() && r.albums.is_empty() && r.playlists.is_empty() {
                // Exactly matching Image 2
                ui.add_space(50.0);
                ui.vertical_centered(|ui| {
                    let center_x = ui.available_rect_before_wrap().center().x;
                    let icon_y = ui.cursor().top() + 20.0;
                    icon_at(ui, pos2(center_x, icon_y), icon::SEARCH, 32.0, theme::text_faint());
                    ui.add_space(48.0);
                    ui.label(egui::RichText::new("No Results").font(theme::bold(16.0)).color(theme::text()));
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(format!("No results for \"{}\"", q)).font(theme::regular(12.5)).color(theme::text_dim()));
                });
                return;
            }

            let heading = |ui: &mut Ui, label: &str| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(label).font(theme::semibold(11.0)).color(theme::text_faint()));
                ui.add_space(2.0);
            };

            let row = |ui: &mut Ui, title: &str, sub: &str, thumbs: &[crate::model::Thumb], round: bool| -> bool {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
                let resp = named(resp, title).on_hover_cursor(egui::CursorIcon::PointingHand);
                let h = if resp.hovered() { 1.0 } else { 0.0 };
                ui.painter().rect_filled(r, 6.0, theme::glass(0.08 * h));
                let art = Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), Vec2::splat(28.0));
                ui.painter().rect_filled(art, if round { 14.0 } else { 4.0 }, theme::glass(0.08));
                if let Some(th) = thumbs.iter().min_by_key(|t| (t.width as i64 - 80).abs()) {
                    super::widgets::paint_remote(ui, &th.url, art, if round { 14.0 } else { 4.0 }, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                let text_w = (r.right() - art.right() - 14.0).max(20.0);
                let line = |s: &str, font: egui::FontId, c: Color32| {
                    let mut job = egui::text::LayoutJob::simple_singleline(s.to_owned(), font, c);
                    job.wrap = egui::text::TextWrapping::truncate_at_width(text_w);
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                ui.painter().galley(pos2(art.right() + 10.0, r.top() + 6.0), line(title, theme::semibold(13.0), theme::text()), theme::text());
                ui.painter().galley(pos2(art.right() + 10.0, r.top() + 23.0), line(sub, theme::regular(11.5), theme::text_dim()), theme::text_dim());
                resp.clicked()
            };

            if !r.tracks.is_empty() {
                heading(ui, "SONGS");
                for (i, tr) in r.tracks.iter().take(5).enumerate() {
                    if row(ui, &tr.title, &tr.artist_line(), &tr.thumbs, false) {
                        app.actions.push(Action::Play(r.tracks.clone(), i));
                    }
                }
            }

            if !r.artists.is_empty() {
                heading(ui, "ARTISTS");
                for c in r.artists.iter().take(3) {
                    if row(ui, &c.title, &super::widgets::card_subtitle(&c.subtitle), &c.thumbs, c.round) {
                        app.actions.push(Action::Open(c.target.clone()));
                    }
                }
            }

            if !r.albums.is_empty() {
                heading(ui, "ALBUMS");
                for c in r.albums.iter().take(3) {
                    if row(ui, &c.title, &super::widgets::card_subtitle(&c.subtitle), &c.thumbs, c.round) {
                        app.actions.push(Action::Open(c.target.clone()));
                    }
                }
            }

            if !r.playlists.is_empty() {
                heading(ui, "PLAYLISTS");
                for c in r.playlists.iter().take(3) {
                    if row(ui, &c.title, &super::widgets::card_subtitle(&c.subtitle), &c.thumbs, c.round) {
                        app.actions.push(Action::Open(c.target.clone()));
                    }
                }
            }
        });
    } else {
        // Suggestions / Searching Menu inside sidebar (Image 1)
        egui::ScrollArea::vertical().id_salt("sidebar-search-suggestions").auto_shrink(false).show(ui, |ui| {
            for (i, s) in app.suggestions.1.clone().into_iter().enumerate() {
                let term = s.text;
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                let resp = named(resp, &term).on_hover_cursor(egui::CursorIcon::PointingHand);
                let h = if resp.contains_pointer() || app.suggest_sel == Some(i) { 1.0 } else { 0.0 };
                ui.painter().rect_filled(r, 6.0, theme::glass(0.08 * h));
                let glyph = if s.forget.is_some() { icon::HISTORY } else { icon::SEARCH };
                icon_at(ui, pos2(r.left() + 14.0, r.center().y), glyph, 14.0, theme::text_dim());
                let room = if s.forget.is_some() { 68.0 } else { 38.0 };
                let text_w = (r.width() - room).max(10.0);

                // Highlight typed matching prefix
                let typed = app.search_text.trim();
                let n = term.chars().zip(typed.chars()).take_while(|(a, b)| a.to_lowercase().eq(b.to_lowercase())).count();
                let split = if n == typed.chars().count() { term.char_indices().nth(n).map_or(term.len(), |(i, _)| i) } else { 0 };
                let (bold, rest) = term.split_at(split);
                let mut job = egui::text::LayoutJob::default();
                job.append(bold, 0.0, egui::TextFormat { font_id: theme::semibold(13.0), color: theme::text(), ..Default::default() });
                job.append(rest, 0.0, egui::TextFormat { font_id: theme::regular(13.0), color: theme::text_dim(), ..Default::default() });
                job.wrap = egui::text::TextWrapping::truncate_at_width(text_w);
                let g = ui.fonts_mut(|f| f.layout_job(job));
                ui.painter().galley(pos2(r.left() + 34.0, r.center().y - 8.0), g, theme::text());

                if let Some(token) = s.forget {
                    let x = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), Vec2::splat(22.0));
                    let xr = named(ui.interact(x, resp.id.with("forget"), Sense::click()), "Remove from recent searches")
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    let xh = if xr.hovered() { 1.0 } else { 0.0 };
                    icon_at(ui, x.center(), icon::TRASH_2, 13.0, theme::lerp_color(theme::text_faint(), theme::text(), xh));
                    if xr.clicked() && !app.forget_held {
                        app.forget_held = true;
                        app.backend.request(crate::backend::Req::ForgetSearch(token.clone()));
                        app.suggestions.1.retain(|s| s.forget.as_deref() != Some(token.as_str()));
                    }
                }

                if resp.clicked() {
                    app.search_text = term.clone();
                    app.search_drop = Some(term.clone());
                    app.suggestions = Default::default();
                    app.search_focused = false;
                    app.actions.push(Action::Go(Page::Search(term)));
                    break;
                }
            }
        });
    }
}

/// A suggestion with what's typed in bold and the rest dimmed, as on music.youtube.com.
#[allow(dead_code)]
fn suggestion_text(ui: &Ui, pos: Pos2, text: &str, typed: &str, width: f32) {
    let n = text.chars().zip(typed.chars()).take_while(|(a, b)| a.to_lowercase().eq(b.to_lowercase())).count();
    let split = if n == typed.chars().count() { text.char_indices().nth(n).map_or(text.len(), |(i, _)| i) } else { 0 };
    let (bold, rest) = text.split_at(split);
    let mut job = egui::text::LayoutJob::default();
    job.append(bold, 0.0, egui::TextFormat { font_id: theme::semibold(15.0), color: theme::text(), ..Default::default() });
    job.append(rest, 0.0, egui::TextFormat { font_id: theme::regular(15.0), color: theme::text_dim(), ..Default::default() });
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(10.0));
    let g = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(pos, g, theme::text());
}

// ------------------------------------------------------------------ player bar

/// The player: one frosted capsule floating over the bottom of the window, as in the
/// BitChord desktop app. Transport on the left, the sleeve with its titles and seek
/// line in the middle, favourites / lyrics / queue / volume on the right.
/// Height of the docked player strip.
pub const DOCKED_H: f32 = 96.0;

/// Whether the theme docks the player to the window's bottom edge.
pub fn docked(app: &App) -> bool {
    app.look().chrome.player_bar == crate::preset::PlayerBar::Docked
}

fn paint_blurred_capsule(painter: &egui::Painter, cap: Rect, radius: f32, tex: &egui::TextureHandle, tint: Color32) {
    let mut mesh = egui::Mesh::with_texture(tex.id());
    let to_uv = |p: Pos2| {
        let ux = ((p.x - cap.left()) / cap.width().max(1.0)).clamp(0.0, 1.0);
        let vy = ((p.y - cap.top()) / cap.height().max(1.0)).clamp(0.0, 1.0);
        pos2(0.10 + ux * 0.80, 0.25 + vy * 0.50)
    };
    mesh.vertices.push(egui::epaint::Vertex { pos: cap.center(), uv: to_uv(cap.center()), color: tint });

    let mut ring: Vec<Pos2> = Vec::with_capacity(28);
    let left_c = pos2(cap.left() + radius, cap.center().y);
    let right_c = pos2(cap.right() - radius, cap.center().y);
    const ARC_SEGS: usize = 12;
    for i in 0..=ARC_SEGS {
        let a = std::f32::consts::FRAC_PI_2 + (i as f32 / ARC_SEGS as f32) * std::f32::consts::PI;
        ring.push(left_c + vec2(a.cos() * radius, a.sin() * radius));
    }
    for i in 0..=ARC_SEGS {
        let a = -std::f32::consts::FRAC_PI_2 + (i as f32 / ARC_SEGS as f32) * std::f32::consts::PI;
        ring.push(right_c + vec2(a.cos() * radius, a.sin() * radius));
    }
    let count = ring.len() as u32;
    for (idx, p) in ring.into_iter().enumerate() {
        mesh.vertices.push(egui::epaint::Vertex { pos: p, uv: to_uv(p), color: tint });
        let cur = 1 + idx as u32;
        let next = 1 + ((idx as u32 + 1) % count);
        mesh.add_triangle(0, cur, next);
    }
    painter.add(egui::Shape::mesh(mesh));
}

const BLUR_VERTEX: &str = r#"
OUT vec2 v_uv;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  v_uv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

const BLUR_FRAG: &str = r#"
precision highp float;
uniform sampler2D u_tex;
uniform vec2 u_texel_step;
IN vec2 v_uv;
#if NEW_SHADER_INTERFACE
out vec4 frag_color;
#define FRAG frag_color
#define TEX texture
#else
#define FRAG gl_FragColor
#define TEX texture2D
#endif
void main() {
  const float w0 = 0.204164;
  const float w1 = 0.180174;
  const float w2 = 0.123832;
  const float w3 = 0.066282;
  const float w4 = 0.027630;
  vec3 c = TEX(u_tex, v_uv).rgb * w0;
  c += (TEX(u_tex, v_uv + u_texel_step * 1.0).rgb + TEX(u_tex, v_uv - u_texel_step * 1.0).rgb) * w1;
  c += (TEX(u_tex, v_uv + u_texel_step * 2.0).rgb + TEX(u_tex, v_uv - u_texel_step * 2.0).rgb) * w2;
  c += (TEX(u_tex, v_uv + u_texel_step * 3.0).rgb + TEX(u_tex, v_uv - u_texel_step * 3.0).rgb) * w3;
  c += (TEX(u_tex, v_uv + u_texel_step * 4.0).rgb + TEX(u_tex, v_uv - u_texel_step * 4.0).rgb) * w4;
  FRAG = vec4(c, 1.0);
}
"#;

const COMP_FRAG: &str = r#"
precision highp float;
uniform sampler2D u_tex;
uniform vec4 u_uv_rect;
uniform vec2 u_rect_size;
uniform float u_radius_px;
uniform vec4 u_tint;
uniform float u_sat;
uniform float u_sheen;
uniform float u_alpha;
IN vec2 v_uv;
#if NEW_SHADER_INTERFACE
out vec4 frag_color;
#define FRAG frag_color
#define TEX texture
#else
#define FRAG gl_FragColor
#define TEX texture2D
#endif
void main() {
  vec2 pos_px = v_uv * u_rect_size;
  vec2 half_sz = u_rect_size * 0.5;
  vec2 q = abs(pos_px - half_sz) - (half_sz - vec2(u_radius_px));
  float dist = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - u_radius_px;
  float edge = (1.0 - smoothstep(-0.75, 0.75, dist)) * u_alpha;
  if (edge <= 0.001) {
    discard;
  }
  vec2 suv = u_uv_rect.xy + v_uv * u_uv_rect.zw;
  vec3 blur_col = TEX(u_tex, suv).rgb;
  float lum = dot(blur_col, vec3(0.299, 0.587, 0.114));
  vec3 vib = mix(vec3(lum), blur_col, u_sat);
  vec3 rgb = mix(vib, u_tint.rgb, u_tint.a);
  float top_sheen = smoothstep(2.5, 0.0, abs(u_rect_size.y - pos_px.y - 1.5)) * smoothstep(-3.0, -1.0, dist);
  float rim = smoothstep(2.2, 0.0, abs(dist + 1.2)) * 0.18;
  rgb += vec3(u_sheen * (top_sheen * 0.85 + rim * 0.75));
  float dither = (fract(sin(dot(pos_px, vec2(12.9898, 78.233))) * 43758.5453) - 0.5) * (1.0 / 255.0);
  rgb = clamp(rgb + dither, 0.0, 1.0);
  FRAG = vec4(rgb * edge, edge);
}
"#;

struct BlurUniforms {
    tex: Option<glow::UniformLocation>,
    step: Option<glow::UniformLocation>,
}

struct CompUniforms {
    tex: Option<glow::UniformLocation>,
    uv_rect: Option<glow::UniformLocation>,
    rect_size: Option<glow::UniformLocation>,
    radius_px: Option<glow::UniformLocation>,
    tint: Option<glow::UniformLocation>,
    sat: Option<glow::UniformLocation>,
    sheen: Option<glow::UniformLocation>,
    alpha: Option<glow::UniformLocation>,
}

struct BackdropBlurGl {
    vao: glow::VertexArray,
    prog_blur: glow::Program,
    blur_loc: BlurUniforms,
    prog_comp: glow::Program,
    comp_loc: CompUniforms,
    tex_src: glow::Texture,
    tex_h: glow::Texture,
    tex_v: glow::Texture,
    fbo_h: glow::Framebuffer,
    fbo_v: glow::Framebuffer,
    src_size: (i32, i32),
    blur_size: (i32, i32),
}

impl BackdropBlurGl {
    unsafe fn link_program(gl: &glow::Context, header: &str, frag_src: &str) -> Option<glow::Program> {
        unsafe {
            let prog = gl.create_program().ok()?;
            let vs = gl.create_shader(glow::VERTEX_SHADER).ok()?;
            let fs = gl.create_shader(glow::FRAGMENT_SHADER).ok()?;
            gl.shader_source(vs, &format!("{header}{BLUR_VERTEX}"));
            gl.compile_shader(vs);
            if !gl.get_shader_compile_status(vs) {
                log::warn!("backdrop blur vs: {}", gl.get_shader_info_log(vs));
                gl.delete_shader(vs);
                gl.delete_shader(fs);
                gl.delete_program(prog);
                return None;
            }
            gl.shader_source(fs, &format!("{header}{frag_src}"));
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(fs) {
                log::warn!("backdrop blur fs: {}", gl.get_shader_info_log(fs));
                gl.delete_shader(vs);
                gl.delete_shader(fs);
                gl.delete_program(prog);
                return None;
            }
            gl.attach_shader(prog, vs);
            gl.attach_shader(prog, fs);
            gl.link_program(prog);
            gl.detach_shader(prog, vs);
            gl.detach_shader(prog, fs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            if !gl.get_program_link_status(prog) {
                log::warn!("backdrop blur link: {}", gl.get_program_info_log(prog));
                gl.delete_program(prog);
                return None;
            }
            Some(prog)
        }
    }

    unsafe fn make_tex(gl: &glow::Context) -> Option<glow::Texture> {
        unsafe {
            let t = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            for (p, v) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, p, v as i32);
            }
            Some(t)
        }
    }

    unsafe fn new(gl: &glow::Context) -> Option<Self> {
        let version = eframe::egui_glow::ShaderVersion::get(gl);
        let header = format!(
            "{}\n#define NEW_SHADER_INTERFACE {}\n{}\n",
            version.version_declaration(),
            version.is_new_shader_interface() as i32,
            if version.is_new_shader_interface() {
                "#define IN in\n#define OUT out"
            } else {
                "#define IN varying\n#define OUT varying"
            }
        );
        unsafe {
            let prog_blur = Self::link_program(gl, &header, BLUR_FRAG)?;
            let prog_comp = Self::link_program(gl, &header, COMP_FRAG)?;
            let vao = gl.create_vertex_array().ok()?;
            let tex_src = Self::make_tex(gl)?;
            let tex_h = Self::make_tex(gl)?;
            let tex_v = Self::make_tex(gl)?;
            let fbo_h = gl.create_framebuffer().ok()?;
            let fbo_v = gl.create_framebuffer().ok()?;
            let blur_loc = BlurUniforms {
                tex: gl.get_uniform_location(prog_blur, "u_tex"),
                step: gl.get_uniform_location(prog_blur, "u_texel_step"),
            };
            let comp_loc = CompUniforms {
                tex: gl.get_uniform_location(prog_comp, "u_tex"),
                uv_rect: gl.get_uniform_location(prog_comp, "u_uv_rect"),
                rect_size: gl.get_uniform_location(prog_comp, "u_rect_size"),
                radius_px: gl.get_uniform_location(prog_comp, "u_radius_px"),
                tint: gl.get_uniform_location(prog_comp, "u_tint"),
                sat: gl.get_uniform_location(prog_comp, "u_sat"),
                sheen: gl.get_uniform_location(prog_comp, "u_sheen"),
                alpha: gl.get_uniform_location(prog_comp, "u_alpha"),
            };
            Some(Self {
                vao,
                prog_blur,
                blur_loc,
                prog_comp,
                comp_loc,
                tex_src,
                tex_h,
                tex_v,
                fbo_h,
                fbo_v,
                src_size: (0, 0),
                blur_size: (0, 0),
            })
        }
    }

    unsafe fn ensure_sizes(&mut self, gl: &glow::Context, src: (i32, i32), blur: (i32, i32)) {
        unsafe {
            if self.src_size != src {
                gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_src));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    src.0,
                    src.1,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                self.src_size = src;
            }
            if self.blur_size != blur {
                for (tex, fbo) in [(self.tex_h, self.fbo_h), (self.tex_v, self.fbo_v)] {
                    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::RGBA8 as i32,
                        blur.0,
                        blur.1,
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(None),
                    );
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
                }
                self.blur_size = blur;
            }
        }
    }
}

#[derive(Default)]
struct BackdropBlurCell {
    gl: Option<BackdropBlurGl>,
    failed: bool,
}

pub(crate) fn paint_live_backdrop_blur(
    ui: &mut Ui,
    id: egui::Id,
    cap: Rect,
    radius: f32,
    tint_col: Color32,
    tint_alpha: f32,
    sat: f32,
    sheen: f32,
    alpha: f32,
) {
    let effective_alpha = (ui.opacity() * alpha).clamp(0.0, 1.0);
    if effective_alpha <= 0.001 {
        return;
    }
    let state: Arc<Mutex<BackdropBlurCell>> = ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_insert_with(id, || Arc::new(Mutex::new(BackdropBlurCell::default())))
            .clone()
    });
    let tint = [
        tint_col.r() as f32 / 255.0,
        tint_col.g() as f32 / 255.0,
        tint_col.b() as f32 / 255.0,
        tint_alpha,
    ];
    let safe_radius = radius.min(cap.width() / 2.0).min(cap.height() / 2.0);
    let cb = eframe::egui_glow::CallbackFn::new(move |info, painter| {
        let vp = info.viewport_in_pixels();
        if vp.width_px <= 0 || vp.height_px <= 0 {
            return;
        }
        let gl = painter.gl();
        let mut cell = state.lock_unpoisoned();
        if cell.failed {
            return;
        }
        unsafe {
            if cell.gl.is_none() {
                match BackdropBlurGl::new(gl) {
                    Some(g) => cell.gl = Some(g),
                    None => {
                        cell.failed = true;
                        return;
                    }
                }
            }
            let Some(s) = cell.gl.as_mut() else { return };

            let prev_draw_fbo = NonZeroU32::new(gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) as u32).map(glow::NativeFramebuffer);
            let prev_read_fbo = NonZeroU32::new(gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING) as u32).map(glow::NativeFramebuffer);
            let prev_scissor = gl.is_enabled(glow::SCISSOR_TEST);
            let prev_blend = gl.is_enabled(glow::BLEND);
            let prev_srgb = gl.is_enabled(glow::FRAMEBUFFER_SRGB);
            let prev_prog = NonZeroU32::new(gl.get_parameter_i32(glow::CURRENT_PROGRAM) as u32).map(glow::NativeProgram);
            let prev_vao = NonZeroU32::new(gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING) as u32).map(glow::NativeVertexArray);
            let prev_active_tex = gl.get_parameter_i32(glow::ACTIVE_TEXTURE) as u32;
            let prev_tex = NonZeroU32::new(gl.get_parameter_i32(glow::TEXTURE_BINDING_2D) as u32).map(glow::NativeTexture);

            let pad_px = (24.0 * info.pixels_per_point).round() as i32;
            let sw = info.screen_size_px[0] as i32;
            let sh = info.screen_size_px[1] as i32;
            let cap_x0 = (vp.left_px - pad_px).clamp(0, sw);
            let cap_y0 = (vp.from_bottom_px - pad_px).clamp(0, sh);
            let cap_x1 = (vp.left_px + vp.width_px + pad_px).clamp(0, sw);
            let cap_y1 = (vp.from_bottom_px + vp.height_px + pad_px).clamp(0, sh);
            let cap_w = (cap_x1 - cap_x0).max(1);
            let cap_h = (cap_y1 - cap_y0).max(1);

            let div = if info.pixels_per_point >= 1.5 { 3 } else { 2 };
            let blur_w = (cap_w / div).max(1);
            let blur_h = (cap_h / div).max(1);
            let step = if info.pixels_per_point >= 1.5 { 1.65 } else { 1.35 };

            gl.active_texture(glow::TEXTURE0);
            s.ensure_sizes(gl, (cap_w, cap_h), (blur_w, blur_h));

            // Capture the rendered screen behind `cap` from the currently bound read FBO.
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, prev_read_fbo);
            gl.bind_texture(glow::TEXTURE_2D, Some(s.tex_src));
            gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, cap_x0, cap_y0, cap_w, cap_h);

            // Two-pass separable Gaussian blur at downsampled resolution.
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            if prev_srgb {
                gl.disable(glow::FRAMEBUFFER_SRGB);
            }
            gl.bind_vertex_array(Some(s.vao));
            gl.use_program(Some(s.prog_blur));
            gl.uniform_1_i32(s.blur_loc.tex.as_ref(), 0);
            gl.viewport(0, 0, blur_w, blur_h);

            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(s.fbo_h));
            gl.bind_texture(glow::TEXTURE_2D, Some(s.tex_src));
            gl.uniform_2_f32(s.blur_loc.step.as_ref(), step / blur_w as f32, 0.0);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(s.fbo_v));
            gl.bind_texture(glow::TEXTURE_2D, Some(s.tex_h));
            gl.uniform_2_f32(s.blur_loc.step.as_ref(), 0.0, step / blur_h as f32);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            // Composite blurred capsule back onto egui's active framebuffer.
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, prev_draw_fbo);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, prev_read_fbo);
            if prev_srgb {
                gl.enable(glow::FRAMEBUFFER_SRGB);
            }
            if prev_scissor {
                gl.enable(glow::SCISSOR_TEST);
            }
            gl.enable(glow::BLEND);
            gl.blend_func_separate(glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.viewport(vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px);

            let u0 = (vp.left_px - cap_x0) as f32 / cap_w as f32;
            let v0 = (vp.from_bottom_px - cap_y0) as f32 / cap_h as f32;
            let uw = vp.width_px as f32 / cap_w as f32;
            let vh = vp.height_px as f32 / cap_h as f32;

            gl.use_program(Some(s.prog_comp));
            gl.bind_texture(glow::TEXTURE_2D, Some(s.tex_v));
            gl.uniform_1_i32(s.comp_loc.tex.as_ref(), 0);
            gl.uniform_4_f32(s.comp_loc.uv_rect.as_ref(), u0, v0, uw, vh);
            gl.uniform_2_f32(s.comp_loc.rect_size.as_ref(), vp.width_px as f32, vp.height_px as f32);
            gl.uniform_1_f32(s.comp_loc.radius_px.as_ref(), safe_radius * info.pixels_per_point);
            gl.uniform_4_f32(s.comp_loc.tint.as_ref(), tint[0], tint[1], tint[2], tint[3]);
            gl.uniform_1_f32(s.comp_loc.sat.as_ref(), sat);
            gl.uniform_1_f32(s.comp_loc.sheen.as_ref(), sheen);
            gl.uniform_1_f32(s.comp_loc.alpha.as_ref(), effective_alpha);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            if !prev_blend {
                gl.disable(glow::BLEND);
            }
            gl.use_program(prev_prog);
            gl.bind_vertex_array(prev_vao);
            gl.bind_texture(glow::TEXTURE_2D, prev_tex);
            gl.active_texture(prev_active_tex);
        }
    });
    ui.painter().add(egui::PaintCallback { rect: cap, callback: Arc::new(cb) });
}

pub fn player_bar(app: &mut App, ui: &mut Ui, now: f64, _frosted: bool) {
    let full = ui.max_rect();
    // A docked strip is opaque and takes clicks across its whole width.
    if docked(app) {
        ui.interact(full, ui.scope_id().with("player-strip"), Sense::hover());
    }
    let ctx = ui.ctx().clone();
    let cap = if docked(app) {
        // Docked: the app's own background, opaque, so rows scrolling under the controls
        // don't show through.
        let cap = Rect::from_min_max(pos2(full.left(), full.bottom() - DOCKED_H), full.max);
        let base = if theme::light() { theme::bg() } else { app.page_color(&ctx) };
        ui.painter().rect_filled(cap, 0.0, base);
        cap.shrink2(vec2(12.0, 0.0))
    } else {
        const H: f32 = 52.0;
        let radius = H / 2.0;
        let max_w = 700.0;
        let w = (full.width() - 80.0).clamp(400.0, max_w).min(full.width() - 24.0);
        let cap = Rect::from_min_size(pos2(full.center().x - w / 2.0, full.bottom() - 14.0 - H), vec2(w, H));
        ui.interact(cap, ui.scope_id().with("player-strip"), Sense::hover());

        let light = theme::light();
        let p_col = app.page_color(&ctx);
        if crate::renderer::active() == crate::renderer::Renderer::OpenGl {
            let (tint_col, tint_alpha, sat, sheen) = if light {
                (theme::lerp_color(p_col, Color32::WHITE, 0.78), 0.56, 1.15, 0.16)
            } else {
                (theme::lerp_color(p_col, Color32::from_rgb(0x16, 0x16, 0x18), 0.55), 0.52, 1.25, 0.07)
            };
            paint_live_backdrop_blur(ui, egui::Id::unique("player-bar-blur-gl"), cap, radius, tint_col, tint_alpha, sat, sheen, 1.0);
        } else {
            let fill = if light {
                theme::with_alpha(theme::lerp_color(p_col, Color32::WHITE, 0.72), 0.72)
            } else {
                theme::with_alpha(theme::lerp_color(p_col, Color32::from_rgb(0x16, 0x16, 0x18), 0.45), 0.72)
            };
            ui.painter().rect_filled(cap, radius, fill);

            let blur_key = match &app.page {
                Page::Album(id) => Some(format!("album:{id}")),
                Page::Artist(id) => Some(format!("artist:{id}")),
                _ => app.art_key.clone().or_else(|| app.current_id().map(str::to_owned)),
            };
            if let Some(k) = blur_key
                && let Some(dom) = app.art_colors.get(&k).copied()
            {
                let r = dom.r() as f32 / 255.0;
                let g = dom.g() as f32 / 255.0;
                let b = dom.b() as f32 / 255.0;
                let mx = r.max(g).max(b);
                let mn = r.min(g).min(b);
                let sat = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
                if sat >= 0.17
                    && mx >= 0.08
                    && let Some(tex) = app.blurred_art_tex(&ctx, &k)
                {
                    let tint = if light {
                        Color32::from_rgba_premultiplied(46, 46, 46, 46)
                    } else {
                        Color32::from_rgba_premultiplied(78, 76, 74, 78)
                    };
                    paint_blurred_capsule(ui.painter(), cap, radius, &tex, tint);
                }
            }
        }

        let stroke_col = if light {
            Color32::from_white_alpha(150)
        } else {
            Color32::from_white_alpha(38)
        };
        ui.painter().rect_stroke(cap, radius, egui::Stroke::new(1.0, stroke_col), egui::StrokeKind::Inside);

        cap
    };

    let status = app.audio.status.clone();
    // An Automix handoff keeps the outgoing song on this strip until the next one is
    // the louder of the two.
    let hold = app.mix_hold().map(|h| (h.total, h.mix, app.mix_hold_pos(h)));
    let track = match &hold {
        Some(_) => app.shown().cloned(),
        None => app.current().cloned(),
    };
    let enabled = track.is_some();
    let playing = app.playing();
    let accent = app.accent_color();
    let cy = cap.center().y;

    let ink = |h: f32| {
        if theme::light() {
            if enabled { Color32::BLACK } else { Color32::from_black_alpha(115) }
        } else {
            let c = theme::lerp_color(theme::text_dim(), theme::text(), h);
            if enabled { c } else { c.gamma_multiply(0.45) }
        }
    };

    // ------------------------------------------------------------------ Left Zone: Transport Controls
    let tbtn = |ui: &mut Ui, x: f32, key: &str, name: &str| -> egui::Response {
        let r = Rect::from_center_size(pos2(x, cy), Vec2::splat(28.0));
        let resp = ui.interact(r, ui.scope_id().with(("bar-t", key)), Sense::click());
        named(resp, name).on_hover_text(name).on_hover_cursor(egui::CursorIcon::PointingHand)
    };

    let left_start = cap.left() + 18.0;
    let show_modes = cap.width() >= 480.0;
    let (x_shuf, x_prev, x_play, x_next, x_rep, left_boundary) = if show_modes {
        let s = left_start + 12.0;
        let p = s + 26.0;
        let pl = p + 28.0;
        let n = pl + 28.0;
        let r = n + 26.0;
        (s, p, pl, n, r, r + 18.0)
    } else {
        let p = left_start + 12.0;
        let pl = p + 28.0;
        let n = pl + 28.0;
        (0.0, p, pl, n, 0.0, n + 18.0)
    };

    // Shuffle (if show_modes)
    if show_modes {
        let r_shuf = tbtn(ui, x_shuf, "shuffle", &t!("shuffle_up_next"));
        let h = if r_shuf.hovered() { 1.0 } else { 0.0 };
        glyphs::draw(ui, r_shuf.rect.center(), 17.0, glyphs::SHUFFLE, ink(h));
        if r_shuf.clicked() {
            app.shuffle_upcoming();
        }
    }

    // Previous
    let r_prev = tbtn(ui, x_prev, "prev", &t!("previous"));
    let h = if r_prev.hovered() { 1.0 } else { 0.0 };
    glyphs::draw(ui, r_prev.rect.center(), 19.5, glyphs::PREVIOUS, ink(h));
    if r_prev.clicked() && enabled {
        app.prev();
    }

    // Play / Pause
    let play_label = if playing { t!("pause") } else { t!("play") };
    let r_play = tbtn(ui, x_play, "play", &play_label);
    let h = if r_play.hovered() { 1.0 } else { 0.0 };
    let c = r_play.rect.center();
    let morph = theme::anim_bool(&ctx, "bar-play-morph", playing || app.buffering, 0.18);
    if app.buffering {
        let a = now as f32 * 6.0;
        let pts: Vec<Pos2> = (0..16).map(|i| c + vec2((a + i as f32 * 0.24).cos(), (a + i as f32 * 0.24).sin()) * 7.5).collect();
        ui.painter().add(egui::Shape::Path(egui::epaint::PathShape::line(pts, egui::epaint::PathStroke::new(2.0, ink(0.6)))));
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    } else {
        let fg = ink(0.9 + 0.1 * h);
        if morph < 1.0 {
            glyphs::draw(ui, c, 22.5, glyphs::PLAY, theme::with_alpha(fg, 1.0 - morph));
        }
        if morph > 0.0 {
            glyphs::draw(ui, c, 22.5, glyphs::PAUSE, theme::with_alpha(fg, morph));
        }
    }
    if r_play.clicked() && enabled {
        app.toggle();
    }

    // Next
    let r_next = tbtn(ui, x_next, "next", &t!("next"));
    let h = if r_next.hovered() { 1.0 } else { 0.0 };
    glyphs::draw(ui, r_next.rect.center(), 19.5, glyphs::NEXT, ink(h));
    if r_next.clicked() && enabled {
        app.next();
    }

    // Repeat (if show_modes)
    if show_modes {
        let (glyph, on) = match app.queue.repeat {
            Repeat::Off => (glyphs::REPEAT, false),
            Repeat::All => (glyphs::REPEAT, true),
            Repeat::One => (glyphs::REPEAT_ONE, true),
        };
        let r_rep = tbtn(ui, x_rep, "repeat", &t!("repeat"));
        let h = if r_rep.hovered() { 1.0 } else { 0.0 };
        glyphs::draw(ui, r_rep.rect.center(), 17.0, glyph, if on { accent } else { ink(h) });
        if r_rep.clicked() {
            app.queue.repeat = match app.queue.repeat {
                Repeat::Off => Repeat::All,
                Repeat::All => Repeat::One,
                Repeat::One => Repeat::Off,
            };
        }
    }

    // ------------------------------------------------------------------ Right Zone: Lyrics, Queue, Star, Volume
    let r_pad = 18.0;
    let r_base = cap.right() - r_pad - 12.0;
    let r_spacing = if cap.width() < 540.0 { 26.0 } else { 28.0 };
    let vol_x = r_base;
    let star_x = r_base - r_spacing;
    let q_x = r_base - 2.0 * r_spacing;
    let lyr_x = r_base - 3.0 * r_spacing;
    let right_boundary = lyr_x - 18.0;

    // Volume hover slider
    let slider_w = (r_spacing * 2.0).min(60.0);
    let r_vol = Rect::from_center_size(pos2(vol_x, cy), Vec2::splat(28.0));
    let slider_rect = Rect::from_min_size(pos2(vol_x - slider_w - 6.0, cy - 8.0), vec2(slider_w, 16.0));

    let hover_icon = ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| r_vol.contains(p));
    let hover_slider = ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| slider_rect.expand2(vec2(6.0, 8.0)).union(r_vol).contains(p));
    let vol_open_id = ui.scope_id().with("bar-vol-open");
    let was_vol_open = ui.data(|d| d.get_temp::<bool>(vol_open_id).unwrap_or(false));
    let is_dragging = was_vol_open && ui.input(|i| i.pointer.any_down());
    let vol_active = hover_icon || (was_vol_open && (hover_slider || is_dragging));
    ui.data_mut(|d| d.insert_temp(vol_open_id, vol_active));
    let v_reveal = theme::anim_bool(&ctx, "bar-vol-reveal-inline", vol_active, 0.16);

    // ------------------------------------------------------------------ Center Zone: Art + Title/Artist + Scrubber + Options
    let center_avail = (right_boundary - left_boundary).max(80.0);
    let center_w = center_avail.min(370.0);
    let center_cx = (left_boundary + right_boundary) / 2.0;
    let center_box = Rect::from_center_size(pos2(center_cx, cy), vec2(center_w, 42.0));

    // Cover art (left edge of center_box)
    let cover_size = 36.0;
    let cover_rect = Rect::from_center_size(pos2(center_box.left() + cover_size / 2.0, center_box.center().y - 1.5), Vec2::splat(cover_size));
    match &track {
        Some(t) => {
            widgets::paint_cover_xfade(ui, egui::Id::unique("player-bar-art"), cover_rect, &t.thumbs, 6.0, 1.0);
            let ar = ui.interact(cover_rect, ui.scope_id().with("bar-art"), Sense::click());
            let ar = named(ar, "Open now playing").on_hover_cursor(egui::CursorIcon::PointingHand);
            let ah = if ar.hovered() { 1.0 } else { 0.0 };
            if ah > 0.0 {
                ui.painter().rect_filled(cover_rect, 6.0, theme::glass(0.16 * ah));
                ui.painter().rect_filled(cover_rect, 6.0, theme::shade(0.3 * ah));
                glyphs::draw(ui, cover_rect.center(), 14.0, glyphs::MAXIMIZE_2, theme::with_alpha(Color32::WHITE, ah));
            }
            if ar.clicked() {
                app.actions.push(Action::OpenNowPlaying(true));
            }
        }
        None => {
            ui.painter().rect_filled(cover_rect, 6.0, theme::glass(0.08));
            paint_mark(ui, cover_rect.shrink(6.0));
        }
    }

    // Options button (...) at the right edge of center_box
    let opt_size = 22.0;
    let r_opt = Rect::from_center_size(pos2(center_box.right() - opt_size / 2.0, center_box.center().y - 1.5), Vec2::splat(opt_size));
    if let Some(t) = &track {
        super::nowplaying::options_menu(app, ui, r_opt, t, false, Some(&ink));
    } else {
        let resp = ui.interact(r_opt, ui.scope_id().with("bar-opt-disabled"), Sense::hover());
        let _ = named(resp, &t!("options"));
        for dx in [-4.5, 0.0, 4.5] {
            ui.painter().circle_filled(r_opt.center() + vec2(dx, 0.0), 1.7, ink(0.0));
        }
    }

    // Text between cover art and options button
    let text_l = cover_rect.right() + 10.0;
    let text_r = r_opt.left() - 6.0;
    let text_w = (text_r - text_l).max(10.0);

    let row1_y = center_box.top() + 9.0;
    let row2_y = center_box.top() + 24.0;

    let (title, artists, album) = match &track {
        Some(t) => (t.title.clone(), t.artists.clone(), t.album.as_ref().map(|a| a.name.clone())),
        None => ("Dawn FM".to_owned(), Vec::new(), None),
    };

    // Row 1: Title (bold)
    let title_col = if theme::light() { Color32::BLACK } else { theme::text() };
    let title_g = widgets::galley(ui, &title, theme::semibold(12.5), title_col, text_w);
    let title_pos = pos2(text_l, row1_y - title_g.size().y / 2.0);
    ui.painter().galley(title_pos, title_g.clone(), title_col);

    let title_hit = Rect::from_min_size(title_pos, vec2(title_g.size().x.min(text_w), 15.0));
    let tre = ui.interact(title_hit, ui.scope_id().with("bar-title"), Sense::click());
    if named(tre, &title).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        app.actions.push(Action::OpenNowPlaying(true));
    }

    // Row 2: Artist — Album (matches Apple Music style in screenshot)
    let mut subtitle = artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
    if let Some(alb) = &album {
        if !subtitle.is_empty() {
            subtitle.push_str(" — ");
        }
        subtitle.push_str(alb);
    }
    let artist_col = if theme::light() { Color32::from_black_alpha(150) } else { theme::text_dim() };
    let sub_g = widgets::galley(ui, &subtitle, theme::regular(11.0), artist_col, text_w);
    let sub_pos = pos2(text_l, row2_y - sub_g.size().y / 2.0);
    ui.painter().galley(sub_pos, sub_g.clone(), artist_col);

    let sub_hit = Rect::from_min_size(sub_pos, vec2(sub_g.size().x.min(text_w), 13.0));
    let sub_resp = ui.interact(sub_hit, ui.scope_id().with("bar-artist"), Sense::click());
    if named(sub_resp, &subtitle).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        if let Some(a0) = artists.first().and_then(|a| a.id.as_ref()) {
            app.actions.push(Action::Open(Target::Artist(a0.clone())));
        } else {
            app.actions.push(Action::OpenNowPlaying(true));
        }
    }

    // Scrubber line along the bottom of center_box
    let held = hold.is_some();
    let (total, mix) = match &hold {
        Some((total, mix, _)) => (*total, *mix),
        None => {
            let total = status
                .duration()
                .map(|d| d.as_secs_f32())
                .or(track.as_ref().and_then(|t| t.duration).map(|d| d as f32))
                .unwrap_or(0.0);
            let next = app.queue.upcoming().is_some() || app.autoplay;
            let mix = match (total > 0.0, next, app.queue.repeat) {
                (true, true, repeat) if repeat != Repeat::One => {
                    status.mix_secs(status.duration().or_else(|| track.as_ref().and_then(|t| t.duration).map(|s| std::time::Duration::from_secs(s as u64)))).min(total * 0.5)
                }
                _ => 0.0,
            };
            (total, mix)
        }
    };
    let smooth = match &hold {
        Some((_, _, pos)) => *pos,
        None => status.position_smooth().as_secs_f32(),
    };
    let pos = app.seek_drag.unwrap_or(smooth).min(total.max(0.0));

    let sx0 = center_box.left();
    let sx1 = center_box.right();
    let sy = center_box.bottom() - 1.0;
    let seek_hit = Rect::from_min_max(pos2(sx0, sy - 7.0), pos2(sx1, sy + 7.0));
    let sresp = ui.interact(seek_hit, ui.scope_id().with("bar-seek"), if held { Sense::hover() } else { Sense::click_and_drag() });
    let sresp = if total > 0.0 && !held { sresp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { sresp };
    let time_label = format!("{} / {}", fmt_time(pos as u32), if total > 0.0 { fmt_time(total as u32) } else { "0:00".to_string() });
    let sresp = named(sresp, &time_label).on_hover_text(&time_label);
    sresp.widget_info(|| egui::WidgetInfo::slider(total > 0.0 && !held, pos as f64, "Seek"));
    let focus = theme::anim_bool(&ctx, "bar-seek-h", total > 0.0 && !held && (sresp.hovered() || sresp.dragged()), 0.15);

    let thick = 2.0 + 1.2 * focus;
    let line_r = Rect::from_min_max(pos2(sx0, sy - thick / 2.0), pos2(sx1, sy + thick / 2.0));
    ui.painter().rect_filled(line_r, thick / 2.0, theme::glass(0.18));

    if mix > 0.5 {
        let edge = sx0 + (sx1 - sx0) * ((total - mix) / total);
        let band = Rect::from_min_max(pos2(edge, line_r.top()), line_r.max);
        ui.painter().rect_filled(band, thick / 2.0, theme::with_alpha(accent, 0.30));
        let tick = Rect::from_min_size(pos2(edge - 0.75, line_r.top()), vec2(1.5, thick));
        ui.painter().rect_filled(tick, 0.75, theme::with_alpha(accent, 0.9));
    }

    let frac = if total > 0.0 { (pos / total).clamp(0.0, 1.0) } else { 0.0 };
    if frac > 0.0 {
        let head_col = if theme::light() { Color32::BLACK } else { theme::with_alpha(theme::text(), 0.85 + 0.15 * focus) };
        ui.painter().rect_filled(Rect::from_min_size(line_r.min, vec2(line_r.width() * frac, thick)), thick / 2.0, head_col);
        if focus > 0.0 {
            let grab_pos = pos2(sx0 + (sx1 - sx0) * frac, sy);
            ui.painter().circle_filled(grab_pos, 3.0 + 0.8 * focus, if theme::light() { Color32::BLACK } else { Color32::WHITE });
        }
    }

    let at = |p: Pos2| (((p.x - sx0) / (sx1 - sx0)).clamp(0.0, 1.0) * total).max(0.0);
    if total > 0.0 && let Some(p) = sresp.interact_pointer_pos() {
        if sresp.dragged() {
            app.seek_drag = Some(at(p));
        }
        if sresp.drag_stopped() || sresp.clicked() {
            app.seek_drag = None;
            app.actions.push(Action::Seek(at(p)));
        }
    }

    // Now render Right Zone: Star, Volume, Lyrics, Queue
    let heard = if held { track.clone() } else { app.playing_track().cloned().or_else(|| track.clone()) };
    let liked = heard.as_ref().is_some_and(|t| app.liked.contains(&t.id));
    let like_label = if liked { t!("remove_from_favourites") } else { t!("add_to_favourites") };
    let r_star = Rect::from_center_size(pos2(star_x, cy), Vec2::splat(28.0));

    // When volume is hovered, draw the volume slider to the left of the volume icon
    if v_reveal > 0.05 {
        volume_slider(app, ui, slider_rect, "bar-vol-inline");
    }

    // Volume icon
    let vol = app.audio.volume();
    let vol_resp = ui.interact(r_vol, ui.scope_id().with(("bar-r", "vol")), Sense::click());
    let vol_resp = named(vol_resp, "Volume").on_hover_cursor(egui::CursorIcon::PointingHand);
    let vol_h = if vol_resp.hovered() || v_reveal > 0.5 { 1.0 } else { 0.0 };
    let step = glyphs::volume_step(vol);
    glyphs::draw(ui, r_vol.center(), 17.5, glyphs::VOLUME[step], ink(vol_h));
    if vol_resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            app.audio.set_volume((vol + scroll / 600.0).clamp(0.0, 1.0));
        }
    }
    if vol_resp.clicked() {
        let stash = unmute_id();
        if vol > 0.0 {
            ui.data_mut(|d| d.insert_temp(stash, vol));
            app.audio.set_volume(0.0);
        } else {
            app.audio.set_volume(ui.data(|d| d.get_temp::<f32>(stash)).unwrap_or(0.6));
        }
    }

    // Star, Lyrics & Queue fade out when volume slider expands
    if v_reveal < 0.95 {
        let r_alpha = (1.0 - v_reveal).clamp(0.0, 1.0);
        let r_ink = |h: f32| theme::with_alpha(ink(h), r_alpha);

        // Star / Like
        let star_resp = ui.interact(r_star, ui.scope_id().with(("bar-r", "like")), Sense::click());
        let star_resp = named(star_resp, &like_label).on_hover_cursor(egui::CursorIcon::PointingHand);
        let star_h = if star_resp.hovered() { 1.0 } else { 0.0 };
        let pop = heard.as_ref().map_or(0.0, |t| theme::anim_bool(&ctx, ("liked", &t.id), liked, 0.3));
        let dim = if heard.is_some() { 1.0 } else { 0.4 };
        let tint = theme::lerp_color(r_ink(star_h), accent, pop).gamma_multiply(dim);
        let size = 17.5 * (1.0 + 0.25 * (pop * std::f32::consts::PI).sin());
        glyphs::draw(ui, r_star.center(), size, glyphs::STAR, tint);
        if pop > 0.0 {
            glyphs::draw(ui, r_star.center(), size, glyphs::STAR_FILLED, theme::with_alpha(tint, tint.a() as f32 / 255.0 * pop));
        }
        if star_resp.clicked() && let Some(t) = heard {
            app.actions.push(Action::Like(t, !liked));
        }

        // Queue / Up Next
        let queue_open = if app.np_open { app.np_tab == NpTab::UpNext } else { app.queue_open };
        let r_q = Rect::from_center_size(pos2(q_x, cy), Vec2::splat(28.0));
        if r_alpha > 0.5 {
            let q_resp = ui.interact(r_q, ui.scope_id().with(("bar-r", "queue")), Sense::click());
            let q_resp = named(q_resp, &t!("up_next")).on_hover_cursor(egui::CursorIcon::PointingHand);
            let q_h = if q_resp.hovered() { 1.0 } else { 0.0 };
            glyphs::draw(ui, r_q.center(), 17.5, glyphs::QUEUE, if queue_open { accent } else { r_ink(q_h) });
            if q_resp.clicked() {
                if app.np_open {
                    app.np_tab = if queue_open { NpTab::Art } else { NpTab::UpNext };
                } else {
                    app.queue_open = !app.queue_open;
                    if app.queue_open {
                        app.lyrics_drawer = false;
                    }
                }
            }
        } else {
            glyphs::draw(ui, r_q.center(), 17.5, glyphs::QUEUE, if queue_open { accent } else { r_ink(0.0) });
        }

        // Lyrics
        let lyrics_on = if app.np_open { app.np_tab == NpTab::Lyrics && !app.no_lyrics() } else { app.lyrics_drawer && !app.queue_open };
        let lyrics_label = format!("{} (⌘L)", t!("lyrics"));
        let r_lyr = Rect::from_center_size(pos2(lyr_x, cy), Vec2::splat(28.0));
        if r_alpha > 0.5 {
            let lyr_resp = ui.interact(r_lyr, ui.scope_id().with(("bar-r", "lyrics")), Sense::click());
            let lyr_resp = named(lyr_resp, &lyrics_label).on_hover_cursor(egui::CursorIcon::PointingHand);
            let lyr_h = if lyr_resp.hovered() { 1.0 } else { 0.0 };
            glyphs::draw(ui, r_lyr.center(), 17.5, if lyrics_on { glyphs::LYRICS } else { glyphs::LYRICS_OFF }, if lyrics_on { accent } else { r_ink(lyr_h) });
            if lyr_resp.clicked() {
                app.toggle_lyrics();
            }
        } else {
            glyphs::draw(ui, r_lyr.center(), 17.5, if lyrics_on { glyphs::LYRICS } else { glyphs::LYRICS_OFF }, if lyrics_on { accent } else { r_ink(0.0) });
        }
    }
}

/// The BitChord mark, white, fitted inside `r` (the sleeve slot when nothing plays).
fn paint_mark(ui: &Ui, r: Rect) {
    mark(ui, r, Color32::from_white_alpha(170));
}

/// The Dawn FM mark (a rising sun), fitted inside `r`, tinted `tint`.
fn mark(ui: &Ui, r: Rect, tint: Color32) {
    const MARK: &[u8] = include_bytes!("../../assets/icon/mark.png");
    const W: f32 = 256.0;
    const H: f32 = 171.0;
    let key = egui::Id::unique("dawnfm-mark");
    let tex = match ui.ctx().data(|d| d.get_temp::<egui::TextureHandle>(key)) {
        Some(t) => t,
        None => {
            let Ok(img) = image::load_from_memory(MARK) else { return };
            let img = img.into_rgba8();
            let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
            let t = ui.ctx().load_texture("dawnfm-mark", color, egui::TextureOptions::LINEAR);
            ui.ctx().data_mut(|d| d.insert_temp(key, t.clone()));
            t
        }
    };
    let k = (r.width() / W).min(r.height() / H);
    let dst = Rect::from_center_size(r.center(), vec2(W * k, H * k));
    ui.painter().image(tex.id(), dst, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
}

/// Where Mute keeps the volume to go back to (the player bar's and the mini player's).
pub fn unmute_id() -> egui::Id {
    egui::Id::unique("unmute")
}

/// The volume track: click or drag to set, scroll over it to nudge.
fn volume_slider(app: &mut App, ui: &mut Ui, slider: Rect, salt: &str) {
    let vol = app.audio.volume();
    let vresp = ui.interact(slider, ui.scope_id().with(salt), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::PointingHand);
    vresp.widget_info(|| egui::WidgetInfo::slider(true, vol as f64, "Volume"));
    let vh = theme::anim_bool(ui.ctx(), ui.scope_id().with((salt, "h")), vresp.hovered() || vresp.dragged(), 0.15);
    let track_r = Rect::from_center_size(slider.center(), vec2(slider.width(), 4.0));
    ui.painter().rect_filled(track_r, 2.0, theme::glass(0.15));
    let fill = Rect::from_min_size(track_r.min, vec2(track_r.width() * vol, 4.0));
    let fill_col = if theme::light() { Color32::BLACK } else { theme::lerp_color(theme::text_dim(), theme::text(), vh) };
    let knob_col = if theme::light() { Color32::BLACK } else { theme::text() };
    ui.painter().rect_filled(fill, 2.0, fill_col);
    ui.painter().circle_filled(pos2(fill.right(), track_r.center().y), 2.0 + 4.5 * vh, knob_col);
    if let Some(p) = vresp.interact_pointer_pos()
        && (vresp.dragged() || vresp.clicked())
    {
        app.audio.set_volume(((p.x - track_r.left()) / track_r.width()).clamp(0.0, 1.0));
    }
    if vresp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            app.audio.set_volume((vol + scroll / 600.0).clamp(0.0, 1.0));
        }
    }
}

// ------------------------------------------------------------------ queue drawer

pub fn queue_drawer(app: &mut App, ui: &mut Ui) {
    let ink = if theme::light() { Color32::BLACK } else { Color32::WHITE };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Up next").font(theme::bold(20.0)).color(ink));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Left of the window buttons, which sit over the drawer's top right.
            ui.add_space(super::titlebar::right_gap(ui.ctx(), ui.max_rect(), 0.0));
            if icon_button(ui, icon::X, 16.0, "Close queue", false).clicked() {
                app.queue_open = false;
            }
        });
    });
    ui.add_space(8.0);
    queue_list(app, ui, "drawer");
}

/// Current track plus everything after it. Shared by the drawer and now playing.
pub fn queue_list(app: &mut App, ui: &mut Ui, salt: &str) {
    let rows = super::pages::Rows::new(app, ui.input(|i| i.time));
    let drag_key = egui::Id::unique(("queue-drag", salt));
    let dragging: Option<usize> = ui.data(|d| d.get_temp(drag_key));
    let mut reorder: Option<(usize, usize)> = None;
    let down = ui.input(|i| i.pointer.any_down());
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let ink = if theme::light() { Color32::BLACK } else { Color32::WHITE };

    egui::ScrollArea::vertical().id_salt(("queue", salt)).auto_shrink(false).show(ui, |ui| {
        if app.queue.tracks.is_empty() {
            ui.label(egui::RichText::new("Nothing queued yet").color(ink));
            return;
        }
        let start = app.queue.index.unwrap_or(0);
        ui.checkbox(&mut app.autoplay, egui::RichText::new("Autoplay similar songs").color(ink)).on_hover_cursor(egui::CursorIcon::PointingHand);
        ui.add_space(6.0);
        let accent = app.accent_color();
        let (queue, liked, acts, has_current, by) = (&app.queue.tracks, &app.liked, &mut app.actions, app.queue.index.is_some(), &app.jam.by);
        widgets::visible_rows(ui, queue.len() - start, |ui, k| {
            let i = start + k;
            let t = &queue[i];
            // The first row is the current song.
            let current = k == 0 && has_current;
            let can_drag = !current;
            let is_dragged = dragging == Some(i);
            let o = widgets::RowOpts {
                current,
                draggable: can_drag,
                is_dragged,
                remove_queue: (!current).then_some(i),
                added_by: by.get(&t.id).cloned(),
                alt: k % 2 == 0,
                plain_text: true,
                ..rows.opts(liked, t, None, false)
            };
            let resp = widgets::track_row(ui, t, &o, acts);
            if can_drag && resp.drag_started() {
                ui.data_mut(|d| d.insert_temp(drag_key, i));
            }
            if let Some(from) = dragging {
                if can_drag && from != i && down && pointer.is_some_and(|p| resp.rect.contains(p)) {
                    reorder = Some((from, i));
                }
                if can_drag && from != i && pointer.is_some_and(|p| resp.rect.contains(p)) {
                    let line_y = if from < i { resp.rect.bottom() } else { resp.rect.top() };
                    ui.painter().hline(resp.rect.x_range(), line_y, egui::Stroke::new(2.0, accent));
                }
            }
            if resp.clicked() && dragging.is_none() {
                // The playing row pauses/resumes instead of doing nothing.
                acts.push(if current { Action::TogglePlay } else { Action::QueueJump(i) });
            }
        });
        if ui.input(|i| i.pointer.any_released()) {
            ui.data_mut(|d| d.remove_temp::<usize>(drag_key));
        }
    });

    if let Some(from) = dragging
        && down
        && let Some(pos) = pointer
        && let Some(t) = app.queue.tracks.get(from)
    {
        let t = t.clone();
        egui::Area::new(egui::Id::unique("queue-drag-preview"))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .fixed_pos(pos + egui::vec2(14.0, 14.0))
            .show(ui.ctx(), |ui| {
                let stroke = egui::Stroke::new(1.0, theme::glass(0.18));
                let shadow = if theme::light() {
                    egui::epaint::Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(45) }
                } else {
                    egui::epaint::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(150) }
                };
                egui::Frame::new()
                    .fill(theme::menu_fill())
                    .corner_radius(10)
                    .stroke(stroke)
                    .shadow(shadow)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        let card_w = 230.0;
                        let card_h = 36.0;
                        let (rect, _) = ui.allocate_exact_size(vec2(card_w, card_h), Sense::hover());
                        let cy = rect.center().y;
                        let img_rect = Rect::from_center_size(pos2(rect.left() + 18.0, cy), Vec2::splat(32.0));
                        widgets::paint_cover(ui, img_rect, &t.thumbs, theme::thumb_radius(), 1.0);

                        let text_x = rect.left() + 40.0;
                        let max_w = card_w - 60.0;
                        widgets::text_at(ui, pos2(text_x, cy - 8.5), &t.title, theme::semibold(13.0), ink, max_w);
                        widgets::text_at(ui, pos2(text_x, cy + 4.5), &t.artist_line(), theme::regular(11.5), ink, max_w);

                        let handle_x = rect.right() - 8.0;
                        let handle_stroke = egui::Stroke::new(1.5, ink);
                        for dy in [-3.5, 0.0, 3.5] {
                            ui.painter().line_segment([pos2(handle_x - 5.0, cy + dy), pos2(handle_x + 5.0, cy + dy)], handle_stroke);
                        }
                    });
            });
    }

    if let Some((from, to)) = reorder {
        app.actions.push(Action::QueueMove(from, to));
        ui.data_mut(|d| d.insert_temp(drag_key, to));
    }
}

// ------------------------------------------------------------------ toast

/// The app's size, as egui's zoom: ⌘+ and ⌘− step it by a tenth and ⌘0 resets it
/// (egui's own shortcuts), and Settings › Appearance has the same steps.
pub const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.7..=1.6;

/// One step larger (`up`) or smaller, on tenths and inside [`ZOOM_RANGE`].
pub fn zoom_step(ctx: &egui::Context, up: bool) {
    let z = ctx.zoom_factor() + if up { 0.1 } else { -0.1 };
    ctx.set_zoom_factor(((z * 10.0).round() / 10.0).clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end()));
}

/// Keeps the size inside its range and, for a moment after it changes, shows it as a
/// percentage at the window's top right.
pub fn zoom_badge(ctx: &egui::Context, now: f64) {
    let z = ctx.zoom_factor();
    if !ZOOM_RANGE.contains(&z) {
        ctx.set_zoom_factor(z.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end()));
    }
    // The size last seen and when it changed; the one at launch isn't a change.
    let id = egui::Id::unique("zoom-badge");
    let (seen, at) = ctx.data(|d| d.get_temp::<(f32, f64)>(id)).unwrap_or((z, f64::NEG_INFINITY));
    let at = if (seen - z).abs() > 0.001 { now } else { at };
    ctx.data_mut(|d| d.insert_temp(id, (z, at)));
    let age = (now - at) as f32;
    if age >= 1.6 {
        return;
    }
    let t = theme::ease_out(age / 0.15) * (1.0 - ((age - 1.3) / 0.3).clamp(0.0, 1.0));
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique("zoom-badge-area"))
        .order(egui::Order::Tooltip)
        .pivot(Align2::RIGHT_TOP)
        .fixed_pos(pos2(screen.right() - 16.0, screen.top() + 14.0))
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_opacity(t);
            let shadow_color = if theme::light() {
                Color32::from_black_alpha(20)
            } else {
                Color32::from_black_alpha(60)
            };
            let stroke = if theme::light() {
                egui::Stroke::new(0.5, Color32::from_black_alpha(25))
            } else {
                egui::Stroke::new(0.5, Color32::from_white_alpha(32))
            };
            egui::Frame::new()
                .fill(theme::menu_fill())
                .corner_radius(6)
                .stroke(stroke)
                .inner_margin(egui::Margin::symmetric(10, 5))
                .shadow(egui::epaint::Shadow { offset: [0, 2], blur: 8, spread: 0, color: shadow_color })
                .show(ui, |ui| {
                    ui.set_min_width(44.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("{:.0}%", z * 100.0))
                                    .font(theme::semibold(12.5))
                                    .color(theme::text()),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
                });
        });
    ctx.request_repaint();
}

pub fn toast(app: &mut App, ctx: &egui::Context, now: f64) {    let Some((msg, shown)) = &app.toast else { return };
    if *shown < 0.0 {
        return;
    }
    let age = (now - shown) as f32;
    let t = theme::ease_out(age / 0.25) * (1.0 - ((age - 4.1) / 0.4).clamp(0.0, 1.0));
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique("toast"))
        .order(egui::Order::Tooltip)
        .pivot(Align2::CENTER_BOTTOM)
        // Above the player bar; in the mini player, over the strip's middle.
        .fixed_pos(Pos2::new(screen.center().x, if screen.height() < 200.0 { screen.center().y + 22.0 } else { screen.bottom() - 104.0 } + 14.0 * (1.0 - t)))
        .show(ctx, |ui| {
            ui.set_opacity(t);
            let stroke = if theme::light() {
                egui::Stroke::new(0.5, Color32::from_black_alpha(25))
            } else {
                egui::Stroke::new(0.5, Color32::from_white_alpha(32))
            };
            egui::Frame::new()
                .fill(theme::menu_fill())
                .corner_radius(10)
                .stroke(stroke)
                .inner_margin(egui::Margin::symmetric(16, 10))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 4],
                    blur: 14,
                    spread: 0,
                    color: if theme::light() { Color32::from_black_alpha(25) } else { Color32::from_black_alpha(70) },
                })
                .show(ui, |ui| {
                    ui.set_max_width(520.0);
                    ui.label(egui::RichText::new(msg.as_str()).font(theme::regular(14.0)).color(theme::text()));
                });
        });
    // Frames only while it fades in or out; in between it just waits.
    if (0.25..4.1).contains(&age) {
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(4.1 - age));
    } else {
        ctx.request_repaint();
    }
}
