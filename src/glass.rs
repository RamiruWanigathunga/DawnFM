//! GPU Liquid Glass renderer, ported from `rust-glass`'s optical shaders and
//! downsampled tent-blur mip chain.
//!
//! Each glass element captures the live framebuffer behind itself (padded for
//! drop shadow and bevel refraction), builds a 6-level tent-filtered pyramid, and
//! draws a continuous-curvature squircle with rim refraction, chromatic
//! dispersion, directional specular highlight, inner glow, and light-tinted
//! shadow.

use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use eframe::glow::{self, HasContext};
use egui::{Color32, Rect, Vec2, vec2};

use crate::sync::MutexExt;

const MIP_LEVELS: usize = 6;

/// Continuous-curvature corner profile, matching `rust-glass`'s Apple corner math.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Corner {
    Radius(f32),
    Capsule,
    Circular(f32),
}

impl From<f32> for Corner {
    fn from(r: f32) -> Self {
        Self::Radius(r)
    }
}

impl Corner {
    /// Returns `(radii, superellipse_power)` for a rectangle of `size`.
    pub fn resolve(self, size: Vec2) -> (Vec2, f32) {
        let min_dim = size.x.min(size.y);
        let half = Vec2::splat(min_dim * 0.5);
        match self {
            Self::Capsule => {
                let r = (half * 1.16).min(size * 0.5);
                let k = ((size.x.max(size.y) / min_dim.max(0.1)) - 1.0).clamp(0.0, 0.3) / 0.3;
                (lerp_vec2(half, r, k), lerp(2.0, 2.18, k))
            }
            Self::Radius(r) => {
                let want = r * 1.35;
                let room = (half.x - r).max(0.0) / (want - r).max(0.01);
                let k = room.clamp(0.0, 1.0);
                (Vec2::splat(lerp(r.min(half.x), want, k).max(0.01)), lerp(2.0, 2.83, k))
            }
            Self::Circular(r) => (Vec2::splat(r.min(half.x).max(0.01)), 2.0),
        }
    }
}

/// Optical parameters of Liquid Glass (lengths in logical points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub refraction: f32,
    pub depth: f32,
    pub outer_refraction: f32,
    pub outer_depth: f32,
    pub pinch: f32,
    pub pinch_width: f32,
    pub sideways: f32,
    pub dispersion: f32,
    pub corner_dispersion: f32,
    pub blur: f32,
    pub magnify: f32,
    pub light_range: (f32, f32),
    pub dark_range: (f32, f32),
    pub saturation: f32,
    pub rim: f32,
    pub rim_width: f32,
    pub rim_base: f32,
    pub dark_edge: f32,
    pub light_spill: f32,
    pub inner_glow: f32,
    pub inner_shadow: f32,
    pub shadow: f32,
    pub shadow_radius: f32,
    pub shadow_y: f32,
    pub shadow_tint: f32,
    pub tint_follow: f32,
}

impl Material {
    /// Controls and floating bars. Balanced legibility and bevel refraction.
    pub const REGULAR: Self = Self {
        refraction: 17.0,
        depth: 13.0,
        outer_refraction: 3.0,
        outer_depth: 4.0,
        pinch: 0.0,
        pinch_width: 20.0,
        sideways: 0.0,
        dispersion: 0.12,
        corner_dispersion: 0.0,
        blur: 4.2,
        magnify: 1.0,
        light_range: (0.72, 1.0),
        dark_range: (0.08, 0.56),
        saturation: 1.45,
        rim: 0.76,
        rim_width: 0.85,
        rim_base: 0.34,
        dark_edge: 0.16,
        light_spill: 0.18,
        inner_glow: 0.16,
        inner_shadow: 0.04,
        shadow: 0.14,
        shadow_radius: 15.0,
        shadow_y: 5.5,
        shadow_tint: 0.40,
        tint_follow: 1.0,
    };

    /// Clear glass for controls floating over album artwork.
    pub const CLEAR: Self = Self {
        refraction: 24.0,
        depth: 18.0,
        outer_refraction: 4.0,
        outer_depth: 4.0,
        dispersion: 0.20,
        blur: 0.8,
        light_range: (0.12, 1.0),
        dark_range: (0.04, 0.92),
        saturation: 1.25,
        rim: 0.85,
        inner_glow: 0.14,
        inner_shadow: 0.06,
        ..Self::REGULAR
    };

    /// Floating sidebars, drawers, and panels.
    pub const THICK: Self = Self {
        refraction: 30.0,
        depth: 22.0,
        outer_refraction: 4.0,
        outer_depth: 5.0,
        dispersion: 0.10,
        blur: 10.5,
        light_range: (0.78, 1.0),
        dark_range: (0.08, 0.48),
        saturation: 1.55,
        shadow: 0.22,
        shadow_radius: 24.0,
        shadow_y: 9.0,
        light_spill: 0.24,
        inner_glow: 0.11,
        ..Self::REGULAR
    };

    /// Clear refractive lens a switch, slider, or segmented thumb morphs into while lifted.
    pub const LENS: Self = Self {
        refraction: 20.0,
        depth: 9.4,
        outer_refraction: 6.0,
        outer_depth: 1.6,
        dispersion: 0.17,
        blur: 0.0,
        light_range: (0.15, 1.0),
        dark_range: (0.06, 1.0),
        saturation: 1.06,
        rim: 1.0,
        rim_width: 0.75,
        rim_base: 0.06,
        dark_edge: 0.92,
        light_spill: 0.0,
        inner_glow: 0.20,
        inner_shadow: 0.09,
        shadow: 0.17,
        shadow_radius: 9.0,
        shadow_y: 5.0,
        shadow_tint: 0.95,
        ..Self::REGULAR
    };

    pub fn scaled(mut self, k: f32) -> Self {
        self.refraction *= k;
        self.depth *= k;
        self.outer_refraction *= k;
        self.outer_depth *= k;
        self.pinch *= k;
        self.pinch_width *= k;
        self.rim_width *= k.max(1.0).sqrt();
        self.shadow_radius *= k;
        self.shadow_y *= k;
        self
    }

    pub fn lerp(&self, o: &Self, t: f32) -> Self {
        let l = |a: f32, b: f32| lerp(a, b, t);
        let l2 = |a: (f32, f32), b: (f32, f32)| (l(a.0, b.0), l(a.1, b.1));
        Self {
            refraction: l(self.refraction, o.refraction),
            depth: l(self.depth, o.depth),
            outer_refraction: l(self.outer_refraction, o.outer_refraction),
            outer_depth: l(self.outer_depth, o.outer_depth),
            pinch: l(self.pinch, o.pinch),
            pinch_width: l(self.pinch_width, o.pinch_width),
            sideways: l(self.sideways, o.sideways),
            dispersion: l(self.dispersion, o.dispersion),
            corner_dispersion: l(self.corner_dispersion, o.corner_dispersion),
            blur: l(self.blur, o.blur),
            magnify: l(self.magnify, o.magnify),
            light_range: l2(self.light_range, o.light_range),
            dark_range: l2(self.dark_range, o.dark_range),
            saturation: l(self.saturation, o.saturation),
            rim: l(self.rim, o.rim),
            rim_width: l(self.rim_width, o.rim_width),
            rim_base: l(self.rim_base, o.rim_base),
            dark_edge: l(self.dark_edge, o.dark_edge),
            light_spill: l(self.light_spill, o.light_spill),
            inner_glow: l(self.inner_glow, o.inner_glow),
            inner_shadow: l(self.inner_shadow, o.inner_shadow),
            shadow: l(self.shadow, o.shadow),
            shadow_radius: l(self.shadow_radius, o.shadow_radius),
            shadow_y: l(self.shadow_y, o.shadow_y),
            shadow_tint: l(self.shadow_tint, o.shadow_tint),
            tint_follow: l(self.tint_follow, o.tint_follow),
        }
    }
}

/// Resolved per-draw styling passed into [`paint`].
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub corner: Corner,
    pub material: Material,
    pub tint: Option<Color32>,
    /// `0.0` = light glass, `1.0` = dark glass.
    pub darkness: f32,
    /// `0.0` = hidden, `1.0` = full glass presence.
    pub alpha: f32,
    /// Specular highlight rotation in radians while pressed/pulled.
    pub light_bend: f32,
    /// Subtle face brightness lift while pressed.
    pub brightness: f32,
}

impl Style {
    pub fn regular(corner: impl Into<Corner>, tint: Option<Color32>, darkness: f32, alpha: f32) -> Self {
        Self { corner: corner.into(), material: Material::REGULAR, tint, darkness, alpha, light_bend: 0.0, brightness: 0.0 }
    }

    pub fn clear(corner: impl Into<Corner>, tint: Option<Color32>, darkness: f32, alpha: f32) -> Self {
        Self { corner: corner.into(), material: Material::CLEAR, tint, darkness, alpha, light_bend: 0.0, brightness: 0.0 }
    }

    pub fn thick(corner: impl Into<Corner>, tint: Option<Color32>, darkness: f32, alpha: f32) -> Self {
        Self { corner: corner.into(), material: Material::THICK, tint, darkness, alpha, light_bend: 0.0, brightness: 0.0 }
    }
}

/// Liquid Glass preset for interactive controls (pills, chips, search boxes, buttons).
pub fn control(corner: impl Into<Corner>, light: bool, hover: f32, alpha: f32) -> Style {
    let h = hover.clamp(0.0, 1.0);
    let tint = if light {
        Color32::from_white_alpha((34.0 + 30.0 * h) as u8)
    } else {
        Color32::from_rgba_unmultiplied(34, 34, 40, (44.0 + 28.0 * h) as u8)
    };
    Style::regular(corner, Some(tint), if light { 0.05 } else { 0.90 }, alpha)
}

/// Liquid Glass preset for floating surfaces (menus, modals, drag previews, floating bars).
pub fn panel(corner: impl Into<Corner>, light: bool, alpha: f32) -> Style {
    let tint = if light {
        Color32::from_rgba_unmultiplied(255, 255, 255, 118)
    } else {
        Color32::from_rgba_unmultiplied(26, 26, 32, 132)
    };
    Style::thick(corner, Some(tint), if light { 0.05 } else { 0.90 }, alpha)
}

/// Paint real Liquid Glass at `rect` into `ui`'s painter.
///
/// Under OpenGL (`glow`), captures the live framebuffer behind `rect`, generates a
/// 6-level tent-filtered pyramid, and runs the optical refraction + dispersion +
/// rim shader. Under non-OpenGL renderers, falls back to a clean soft frosted fill.
pub fn paint(ui: &egui::Ui, id: egui::Id, rect: Rect, style: Style) {
    ui.painter().add(shape(ui, id, rect, style));
}

/// Replace a previously reserved [`egui::layers::ShapeIdx`] with Liquid Glass for `rect`.
pub fn paint_at(ui: &egui::Ui, slot: egui::layers::ShapeIdx, id: egui::Id, rect: Rect, style: Style) {
    ui.painter().set(slot, shape(ui, id, rect, style));
}

/// Paint Liquid Glass behind dynamic `add_contents` layout (or `fallback` on non-OpenGL).
pub fn surface<R>(
    ui: &mut egui::Ui,
    id: egui::Id,
    margin: impl Into<egui::Margin>,
    style: Style,
    fallback: egui::Frame,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    if crate::renderer::active() == crate::renderer::Renderer::OpenGl {
        let slot = ui.painter().add(egui::Shape::Noop);
        let inner = egui::Frame::NONE.inner_margin(margin).show(ui, add_contents);
        paint_at(ui, slot, id, inner.response.rect, style);
        inner
    } else {
        fallback.inner_margin(margin).show(ui, add_contents)
    }
}

/// Build an [`egui::Shape`] rendering Liquid Glass at `rect` (or fallback/Noop).
pub fn shape(ui: &egui::Ui, id: egui::Id, rect: Rect, style: Style) -> egui::Shape {
    let presence = (ui.opacity() * style.alpha).clamp(0.0, 1.0);
    if presence <= 0.001 || !rect.is_positive() {
        return egui::Shape::Noop;
    }
    if crate::renderer::active() != crate::renderer::Renderer::OpenGl {
        return fallback_shape(rect, style, presence);
    }

    let m = style.material;
    let pad_pt = (m.shadow_radius * 1.4 + m.shadow_y.abs() + 4.0).max(16.0);
    let draw_rect = rect.expand(pad_pt).intersect(ui.clip_rect());
    if !draw_rect.is_positive() {
        return egui::Shape::Noop;
    }

    let prog_cell: Arc<Mutex<GlPrograms>> = ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_insert_with(egui::Id::unique("liquid-glass-programs"), || Arc::new(Mutex::new(GlPrograms::default())))
            .clone()
    });
    let target_cell: Arc<Mutex<GlassTarget>> = ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_insert_with(id, || Arc::new(Mutex::new(GlassTarget::default())))
            .clone()
    });

    let size = rect.size().max(Vec2::splat(1.0));
    let (rad_pt, power) = style.corner.resolve(size);
    let min_half_pt = size.x.min(size.y) * 0.5;
    let max_depth_pt = min_half_pt * 0.98;
    let k = (max_depth_pt / m.depth.max(0.1)).min(1.0);
    let d = style.darkness.clamp(0.0, 1.0);
    let face_lo = (lerp(m.light_range.0, m.dark_range.0, d) + style.brightness).clamp(0.0, 1.2);
    let face_hi = (lerp(m.light_range.1, m.dark_range.1, d) + style.brightness).clamp(0.0, 1.3);
    let shadow_op = m.shadow * lerp(0.28, 1.0, d);
    let shadow_tint = lerp(0.75, m.shadow_tint, d);
    let dark_edge = (m.dark_edge * lerp(0.22, 1.0, d)).min(0.9);
    let inner_shadow = m.inner_shadow * lerp(0.30, 1.0, d);
    let (bs, bc) = style.light_bend.sin_cos();
    let light_dir = [-0.45 * bc - (-0.893) * bs, -0.45 * bs + (-0.893) * bc];
    let tint = style.tint.map_or([0.0; 4], |c| {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        [
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ]
    });

    let cb = eframe::egui_glow::CallbackFn::new(move |info, painter| {
        let vp = info.viewport_in_pixels();
        if vp.width_px <= 0 || vp.height_px <= 0 {
            return;
        }
        let gl = painter.gl();
        let mut progs = prog_cell.lock_unpoisoned();
        if progs.failed {
            return;
        }
        let mut target = target_cell.lock_unpoisoned();
        unsafe {
            if progs.state.is_none() {
                match GlProgramState::new(gl) {
                    Some(p) => progs.state = Some(p),
                    None => {
                        progs.failed = true;
                        return;
                    }
                }
            }
            let Some(p) = progs.state.as_ref() else { return };

            let prev_draw_fbo = NonZeroU32::new(gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) as u32).map(glow::NativeFramebuffer);
            let prev_read_fbo = NonZeroU32::new(gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING) as u32).map(glow::NativeFramebuffer);
            let prev_scissor = gl.is_enabled(glow::SCISSOR_TEST);
            let prev_blend = gl.is_enabled(glow::BLEND);
            let prev_srgb = gl.is_enabled(glow::FRAMEBUFFER_SRGB);
            let prev_prog = NonZeroU32::new(gl.get_parameter_i32(glow::CURRENT_PROGRAM) as u32).map(glow::NativeProgram);
            let prev_vao = NonZeroU32::new(gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING) as u32).map(glow::NativeVertexArray);
            let prev_active_tex = gl.get_parameter_i32(glow::ACTIVE_TEXTURE) as u32;
            let prev_tex0 = bound_tex_on(gl, glow::TEXTURE0);

            let ppp = info.pixels_per_point;
            let sw = info.screen_size_px[0] as i32;
            let sh = info.screen_size_px[1] as i32;
            let extra_px = (m.refraction.max(m.outer_refraction).max(16.0) * ppp).ceil() as i32;
            let cap_x0 = (vp.left_px - extra_px).clamp(0, sw);
            let cap_y0 = (vp.from_bottom_px - extra_px).clamp(0, sh);
            let cap_x1 = (vp.left_px + vp.width_px + extra_px).clamp(0, sw);
            let cap_y1 = (vp.from_bottom_px + vp.height_px + extra_px).clamp(0, sh);
            let cap_w = (cap_x1 - cap_x0).max(1);
            let cap_h = (cap_y1 - cap_y0).max(1);

            if !target.ensure_sizes(gl, cap_w, cap_h) {
                return;
            }

            // 1. Capture live backdrop behind the element + padding.
            gl.active_texture(glow::TEXTURE0);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, prev_read_fbo);
            gl.bind_texture(glow::TEXTURE_2D, Some(target.mips[0]));
            gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, cap_x0, cap_y0, cap_w, cap_h);

            // 2. Build 6-level tent-filter mip pyramid.
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            if prev_srgb {
                gl.disable(glow::FRAMEBUFFER_SRGB);
            }
            gl.bind_vertex_array(Some(p.vao));
            gl.use_program(Some(p.prog_tent));
            gl.uniform_1_i32(p.tent_tex.as_ref(), 0);
            for lvl in 1..MIP_LEVELS {
                let (sw_src, sh_src) = target.sizes[lvl - 1];
                let (mw, mh) = target.sizes[lvl];
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.fbos[lvl - 1]));
                gl.viewport(0, 0, mw, mh);
                gl.bind_texture(glow::TEXTURE_2D, Some(target.mips[lvl - 1]));
                gl.uniform_2_f32(p.tent_src_size.as_ref(), sw_src as f32, sh_src as f32);
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
            }

            // 3. Draw Liquid Glass over `vp` (aligned in capture-local y-down coordinates).
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

            let cap_top_y = (sh - cap_y1) as f32;
            let vp_top_y = (sh - (vp.from_bottom_px + vp.height_px)) as f32;
            let vp_in_cap_x = (vp.left_px - cap_x0) as f32;
            let vp_in_cap_y = vp_top_y - cap_top_y;

            let ex0 = rect.left() * ppp - cap_x0 as f32;
            let ey0 = rect.top() * ppp - cap_top_y;
            let ex1 = rect.right() * ppp - cap_x0 as f32;
            let ey1 = rect.bottom() * ppp - cap_top_y;
            let center = [(ex0 + ex1) * 0.5, (ey0 + ey1) * 0.5];
            let half = [(ex1 - ex0) * 0.5, (ey1 - ey0) * 0.5];

            let u = &p.glass_u;
            gl.use_program(Some(p.prog_glass));
            for (lvl, tex) in target.mips.iter().enumerate() {
                gl.active_texture(glow::TEXTURE0 + lvl as u32);
                gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
                gl.uniform_1_i32(u.mips[lvl].as_ref(), lvl as i32);
                let (mw, mh) = target.sizes[lvl];
                gl.uniform_2_f32(u.mip_sizes[lvl].as_ref(), mw as f32, mh as f32);
            }
            gl.uniform_4_f32(u.vp_in_cap.as_ref(), vp_in_cap_x, vp_in_cap_y, vp.width_px as f32, vp.height_px as f32);
            gl.uniform_2_f32(u.cap_size.as_ref(), cap_w as f32, cap_h as f32);
            gl.uniform_2_f32(u.center.as_ref(), center[0], center[1]);
            gl.uniform_2_f32(u.half.as_ref(), half[0].max(0.5), half[1].max(0.5));
            gl.uniform_2_f32(u.radius.as_ref(), rad_pt.x * ppp, rad_pt.y * ppp);
            gl.uniform_1_f32(u.power.as_ref(), power);
            gl.uniform_1_f32(u.ppp.as_ref(), ppp);
            gl.uniform_2_f32(u.light.as_ref(), light_dir[0], light_dir[1]);

            gl.uniform_4_f32(u.tint.as_ref(), tint[0], tint[1], tint[2], tint[3]);
            gl.uniform_1_f32(u.tint_follow.as_ref(), m.tint_follow);
            gl.uniform_1_f32(u.sideways.as_ref(), m.sideways);
            gl.uniform_1_f32(u.corner_disp.as_ref(), m.corner_dispersion);
            gl.uniform_1_f32(u.opacity.as_ref(), presence);

            gl.uniform_4_f32(
                u.lens.as_ref(),
                m.refraction * k.max(0.35) * presence * ppp,
                m.depth * k * ppp,
                m.outer_refraction * k * presence * ppp,
                m.outer_depth * k * ppp,
            );
            gl.uniform_4_f32(
                u.optics.as_ref(),
                m.dispersion,
                m.blur * ppp * presence.sqrt(),
                m.magnify,
                m.saturation,
            );
            gl.uniform_4_f32(
                u.pinch.as_ref(),
                m.pinch * k * presence * ppp,
                m.pinch_width * k * ppp,
                face_lo,
                face_hi,
            );
            gl.uniform_4_f32(
                u.edge.as_ref(),
                (m.rim * lerp(1.0, 0.72, d)).min(1.0),
                m.rim_width * ppp.max(1.0),
                m.rim_base,
                dark_edge,
            );
            gl.uniform_4_f32(
                u.glow.as_ref(),
                m.light_spill * presence,
                m.inner_glow * lerp(1.0, 0.6, d),
                inner_shadow,
                0.0,
            );
            gl.uniform_4_f32(
                u.shadow.as_ref(),
                shadow_op * presence,
                m.shadow_radius * ppp,
                m.shadow_y * ppp,
                shadow_tint,
            );
            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            for lvl in (1..MIP_LEVELS).rev() {
                gl.active_texture(glow::TEXTURE0 + lvl as u32);
                gl.bind_texture(glow::TEXTURE_2D, None);
            }
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, prev_tex0);
            gl.active_texture(prev_active_tex);
            if !prev_blend {
                gl.disable(glow::BLEND);
            }
            gl.use_program(prev_prog);
            gl.bind_vertex_array(prev_vao);
        }
    });

    egui::PaintCallback { rect: draw_rect, callback: Arc::new(cb) }.into()
}

fn fallback_shape(rect: Rect, style: Style, presence: f32) -> egui::Shape {
    let size = rect.size().max(Vec2::splat(1.0));
    let (rad, _) = style.corner.resolve(size);
    let r = rad.x.min(rad.y).clamp(0.0, 255.0) as u8;
    let base = if let Some(t) = style.tint {
        t.gamma_multiply(presence)
    } else if style.darkness < 0.5 {
        Color32::from_white_alpha((175.0 * presence) as u8)
    } else {
        Color32::from_rgba_unmultiplied(28, 28, 32, (175.0 * presence) as u8)
    };
    egui::Shape::rect_filled(rect, r, base)
}

unsafe fn bound_tex_on(gl: &glow::Context, unit: u32) -> Option<glow::NativeTexture> {
    unsafe {
        gl.active_texture(unit);
        NonZeroU32::new(gl.get_parameter_i32(glow::TEXTURE_BINDING_2D) as u32).map(glow::NativeTexture)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_vec2(a: Vec2, b: Vec2, t: f32) -> Vec2 {
    vec2(lerp(a.x, b.x, t), lerp(a.y, b.y, t))
}

#[derive(Default)]
struct GlPrograms {
    state: Option<GlProgramState>,
    failed: bool,
}

struct GlassUniforms {
    mips: [Option<glow::UniformLocation>; MIP_LEVELS],
    mip_sizes: [Option<glow::UniformLocation>; MIP_LEVELS],
    vp_in_cap: Option<glow::UniformLocation>,
    cap_size: Option<glow::UniformLocation>,
    center: Option<glow::UniformLocation>,
    half: Option<glow::UniformLocation>,
    radius: Option<glow::UniformLocation>,
    power: Option<glow::UniformLocation>,
    ppp: Option<glow::UniformLocation>,
    light: Option<glow::UniformLocation>,
    tint: Option<glow::UniformLocation>,
    tint_follow: Option<glow::UniformLocation>,
    sideways: Option<glow::UniformLocation>,
    corner_disp: Option<glow::UniformLocation>,
    opacity: Option<glow::UniformLocation>,
    lens: Option<glow::UniformLocation>,
    optics: Option<glow::UniformLocation>,
    pinch: Option<glow::UniformLocation>,
    edge: Option<glow::UniformLocation>,
    glow: Option<glow::UniformLocation>,
    shadow: Option<glow::UniformLocation>,
}

struct GlProgramState {
    vao: glow::VertexArray,
    prog_tent: glow::Program,
    tent_tex: Option<glow::UniformLocation>,
    tent_src_size: Option<glow::UniformLocation>,
    prog_glass: glow::Program,
    glass_u: GlassUniforms,
}

impl GlProgramState {
    unsafe fn link(gl: &glow::Context, header: &str, vert_src: &str, frag_src: &str) -> Option<glow::Program> {
        unsafe {
            let prog = gl.create_program().ok()?;
            let vs = gl.create_shader(glow::VERTEX_SHADER).ok()?;
            let fs = gl.create_shader(glow::FRAGMENT_SHADER).ok()?;
            gl.shader_source(vs, &format!("{header}{vert_src}"));
            gl.compile_shader(vs);
            if !gl.get_shader_compile_status(vs) {
                log::warn!("liquid glass vs: {}", gl.get_shader_info_log(vs));
                gl.delete_shader(vs);
                gl.delete_shader(fs);
                gl.delete_program(prog);
                return None;
            }
            gl.shader_source(fs, &format!("{header}{frag_src}"));
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(fs) {
                log::warn!("liquid glass fs: {}", gl.get_shader_info_log(fs));
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
                log::warn!("liquid glass link: {}", gl.get_program_info_log(prog));
                gl.delete_program(prog);
                return None;
            }
            Some(prog)
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
            let prog_tent = Self::link(gl, &header, TENT_VERT, TENT_FRAG)?;
            let prog_glass = Self::link(gl, &header, GLASS_VERT, GLASS_FRAG)?;
            let vao = gl.create_vertex_array().ok()?;
            let tent_tex = gl.get_uniform_location(prog_tent, "u_src");
            let tent_src_size = gl.get_uniform_location(prog_tent, "u_src_size");
            let loc = |name: &str| gl.get_uniform_location(prog_glass, name);
            let mips = [
                loc("u_mip0"),
                loc("u_mip1"),
                loc("u_mip2"),
                loc("u_mip3"),
                loc("u_mip4"),
                loc("u_mip5"),
            ];
            let mip_sizes = [
                loc("u_mip_size0"),
                loc("u_mip_size1"),
                loc("u_mip_size2"),
                loc("u_mip_size3"),
                loc("u_mip_size4"),
                loc("u_mip_size5"),
            ];
            let glass_u = GlassUniforms {
                mips,
                mip_sizes,
                vp_in_cap: loc("u_vp_in_cap"),
                cap_size: loc("u_cap_size"),
                center: loc("u_center"),
                half: loc("u_half"),
                radius: loc("u_radius"),
                power: loc("u_power"),
                ppp: loc("u_ppp"),
                light: loc("u_light"),
                tint: loc("u_tint"),
                tint_follow: loc("u_tint_follow"),
                sideways: loc("u_sideways"),
                corner_disp: loc("u_corner_disp"),
                opacity: loc("u_opacity"),
                lens: loc("u_lens"),
                optics: loc("u_optics"),
                pinch: loc("u_pinch"),
                edge: loc("u_edge"),
                glow: loc("u_glow"),
                shadow: loc("u_shadow"),
            };
            Some(Self { vao, prog_tent, tent_tex, tent_src_size, prog_glass, glass_u })
        }
    }
}

#[derive(Default)]
struct GlassTarget {
    mips: Vec<glow::Texture>,
    fbos: Vec<glow::Framebuffer>,
    sizes: Vec<(i32, i32)>,
    cap_size: (i32, i32),
}

impl GlassTarget {
    unsafe fn ensure_sizes(&mut self, gl: &glow::Context, cap_w: i32, cap_h: i32) -> bool {
        unsafe {
            if self.mips.is_empty() {
                for _ in 0..MIP_LEVELS {
                    let Ok(t) = gl.create_texture() else { return false };
                    gl.bind_texture(glow::TEXTURE_2D, Some(t));
                    for (p, v) in [
                        (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                        (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                        (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                        (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                    ] {
                        gl.tex_parameter_i32(glow::TEXTURE_2D, p, v as i32);
                    }
                    self.mips.push(t);
                }
                for _ in 1..MIP_LEVELS {
                    let Ok(f) = gl.create_framebuffer() else { return false };
                    self.fbos.push(f);
                }
            }
            if self.cap_size == (cap_w, cap_h) {
                return true;
            }
            self.sizes.clear();
            let (mut w, mut h) = (cap_w, cap_h);
            for lvl in 0..MIP_LEVELS {
                if lvl > 0 {
                    w = (w / 2).max(1);
                    h = (h / 2).max(1);
                }
                self.sizes.push((w, h));
                gl.bind_texture(glow::TEXTURE_2D, Some(self.mips[lvl]));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    w,
                    h,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                if lvl > 0 {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbos[lvl - 1]));
                    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(self.mips[lvl]), 0);
                }
            }
            self.cap_size = (cap_w, cap_h);
            true
        }
    }
}

const TENT_VERT: &str = r#"
OUT vec2 v_uv;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  v_uv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

const TENT_FRAG: &str = r#"
precision highp float;
uniform sampler2D u_src;
uniform vec2 u_src_size;
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
  vec2 px = 1.0 / max(u_src_size, vec2(1.0));
  vec3 c = TEX(u_src, v_uv).rgb * 4.0;
  c += TEX(u_src, v_uv + px * vec2(-1.0, -1.0)).rgb;
  c += TEX(u_src, v_uv + px * vec2( 1.0, -1.0)).rgb;
  c += TEX(u_src, v_uv + px * vec2(-1.0,  1.0)).rgb;
  c += TEX(u_src, v_uv + px * vec2( 1.0,  1.0)).rgb;
  FRAG = vec4(c / 8.0, 1.0);
}
"#;

const GLASS_VERT: &str = r#"
uniform vec4 u_vp_in_cap;
OUT vec2 v_pos;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  v_pos = vec2(
    u_vp_in_cap.x + p.x * u_vp_in_cap.z,
    u_vp_in_cap.y + (1.0 - p.y) * u_vp_in_cap.w
  );
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

const GLASS_FRAG: &str = r#"
precision highp float;
uniform sampler2D u_mip0;
uniform sampler2D u_mip1;
uniform sampler2D u_mip2;
uniform sampler2D u_mip3;
uniform sampler2D u_mip4;
uniform sampler2D u_mip5;
uniform vec2 u_mip_size0;
uniform vec2 u_mip_size1;
uniform vec2 u_mip_size2;
uniform vec2 u_mip_size3;
uniform vec2 u_mip_size4;
uniform vec2 u_mip_size5;
uniform vec2 u_cap_size;
uniform vec2 u_center;
uniform vec2 u_half;
uniform vec2 u_radius;
uniform float u_power;
uniform float u_ppp;
uniform vec2 u_light;
uniform vec4 u_tint;
uniform float u_tint_follow;
uniform float u_sideways;
uniform float u_corner_disp;
uniform float u_opacity;
uniform vec4 u_lens;    // inner_amount, inner_height, outer_amount, outer_height
uniform vec4 u_optics;  // dispersion, blur, magnify, saturation
uniform vec4 u_pinch;   // pinch, pinch_width, face_lo, face_hi
uniform vec4 u_edge;    // rim, rim_width, rim_base, dark_edge
uniform vec4 u_glow;    // light_spill, inner_glow, inner_shadow, pad
uniform vec4 u_shadow;  // shadow, shadow_radius, shadow_y, shadow_tint
IN vec2 v_pos;
#if NEW_SHADER_INTERFACE
out vec4 frag_color;
#define FRAG frag_color
#define TEX texture
#else
#define FRAG gl_FragColor
#define TEX texture2D
#endif

float luma(vec3 c) {
  return dot(c, vec3(0.299, 0.587, 0.114));
}

float gauss(float x) {
  return exp(-x * x);
}

float edge_aa(float d) {
  return smoothstep(0.72, -0.72, d);
}

// Signed distance & outward normal of a squircle / continuous-curvature rounded rect.
vec3 sd_squircle(vec2 p, vec2 half_sz, vec2 r_in, float n) {
  vec2 r = max(min(r_in, half_sz), vec2(0.001));
  vec2 s = vec2(p.x >= 0.0 ? 1.0 : -1.0, p.y >= 0.0 ? 1.0 : -1.0);
  vec2 q = abs(p) - (half_sz - r);
  if (q.x > 0.0 && q.y > 0.0) {
    vec2 u = max(q / r, vec2(1e-5));
    float l = pow(pow(u.x, n) + pow(u.y, n), 1.0 / n);
    float k = pow(max(l, 1e-5), 1.0 - n);
    vec2 g = vec2(pow(u.x, n - 1.0), pow(u.y, n - 1.0)) * k / r;
    float gl = max(length(g), 1e-5);
    return vec3((l - 1.0) / gl, s * g / gl);
  }
  vec2 e = q - r;
  if (e.x > e.y) {
    return vec3(e.x, s.x, 0.0);
  }
  return vec3(e.y, 0.0, s.y);
}

float bevel(float t) {
  float u = 1.0 - clamp(t, 0.0, 1.0);
  return 1.0 - sqrt(max(1.0 - u * u, 0.0));
}

float lens(float depth, float side, float w) {
  float wide = clamp(1.0 + 1.5 * w, 0.4, 3.0);
  float d = u_lens.z * bevel(depth / max(u_lens.w, 0.5))
          - u_lens.x * mix(1.0, side, u_sideways) * bevel(depth / max(u_lens.y * wide, 0.5));
  if (u_pinch.x > 0.0) {
    float h = sin(3.14159265 * clamp(depth / max(u_pinch.y, 1.0), 0.0, 1.0));
    d += u_pinch.x * (1.0 + 0.3 * w) * mix(1.0, 1.0 - side, u_sideways) * h * h;
  }
  return d;
}

vec3 sample_mip(int lvl, vec2 uv) {
  if (lvl <= 0) return TEX(u_mip0, uv).rgb;
  if (lvl == 1) return TEX(u_mip1, uv).rgb;
  if (lvl == 2) return TEX(u_mip2, uv).rgb;
  if (lvl == 3) return TEX(u_mip3, uv).rgb;
  if (lvl == 4) return TEX(u_mip4, uv).rgb;
  return TEX(u_mip5, uv).rgb;
}

vec2 mip_dim(int lvl) {
  if (lvl <= 0) return u_mip_size0;
  if (lvl == 1) return u_mip_size1;
  if (lvl == 2) return u_mip_size2;
  if (lvl == 3) return u_mip_size3;
  if (lvl == 4) return u_mip_size4;
  return u_mip_size5;
}

vec3 bicubic(vec2 uv, int lvl) {
  vec2 dim = mip_dim(lvl);
  vec2 x = uv * dim - 0.5;
  vec2 f = fract(x);
  vec2 i = floor(x);
  vec2 w0 = (1.0 - f) * (1.0 - f) * (1.0 - f) / 6.0;
  vec2 w1 = (4.0 - 6.0 * f * f + 3.0 * f * f * f) / 6.0;
  vec2 w3 = f * f * f / 6.0;
  vec2 w2 = 1.0 - w0 - w1 - w3;
  vec2 g0 = w0 + w1;
  vec2 g1 = w2 + w3;
  vec2 h0 = (i - 0.5 + w1 / max(g0, vec2(1e-5))) / dim;
  vec2 h1 = (i + 1.5 + w3 / max(g1, vec2(1e-5))) / dim;
  vec3 a = sample_mip(lvl, vec2(h0.x, h0.y));
  vec3 b = sample_mip(lvl, vec2(h1.x, h0.y));
  vec3 c = sample_mip(lvl, vec2(h0.x, h1.y));
  vec3 d = sample_mip(lvl, vec2(h1.x, h1.y));
  return mix(mix(d, c, g0.x), mix(b, a, g0.x), g0.y);
}

vec2 cap_uv(vec2 p) {
  vec2 q = clamp(p, vec2(0.5), u_cap_size - 0.5);
  return vec2(q.x / u_cap_size.x, 1.0 - q.y / u_cap_size.y);
}

vec3 sample_bg(vec2 p, float r) {
  vec2 uv = cap_uv(p);
  if (r < 0.35) {
    return sample_mip(0, uv);
  }
  float lod = clamp(log2(1.0 + r * 0.75), 0.0, 5.0);
  int l0 = int(floor(lod));
  vec3 a = (l0 == 0) ? sample_mip(0, uv) : bicubic(uv, l0);
  float frac = lod - float(l0);
  if (frac < 0.02 || l0 >= 5) {
    return a;
  }
  return mix(a, bicubic(uv, min(l0 + 1, 5)), frac);
}

vec3 sample_wide(vec2 p, float r) {
  vec2 uv = cap_uv(p);
  int lvl = int(clamp(floor(log2(1.0 + r * 0.75) + 0.5), 0.0, 5.0));
  return (lvl == 0) ? sample_mip(0, uv) : bicubic(uv, lvl);
}

vec3 vibrance(vec3 c, float s) {
  return max(mix(vec3(luma(c)), c, s), vec3(0.0));
}

void main() {
  vec2 p = v_pos;
  vec3 f = sd_squircle(p - u_center, u_half, u_radius, u_power);
  float d = f.x;
  vec2 n = f.yz;
  float cov = edge_aa(d);
  float half_min = min(u_half.x, u_half.y);

  // Outer shadow tinted by light passing through the glass.
  vec4 under = vec4(0.0);
  if (cov < 1.0 && u_shadow.x > 0.001) {
    float ds = sd_squircle(p - vec2(0.0, u_shadow.z) - u_center, u_half, u_radius, u_power).x;
    float soft = 1.0 / (1.0 + exp(clamp(ds / max(u_shadow.y * 0.38, 0.5), -20.0, 20.0)));
    float contact = exp(-max(d, 0.0) / max(1.2 * u_ppp, 0.5)) * min(u_edge.w, 0.3) * 0.30;
    float a = clamp(u_shadow.x * soft + contact * u_opacity, 0.0, 1.0);
    vec3 rgb = vec3(0.0);
    if (u_shadow.w > 0.0 && a > 0.004) {
      vec2 q = p - n * (max(d, 0.0) + half_min * 0.95);
      float spread_px = max(half_min * 1.6, 10.0 * u_ppp);
      vec3 bg = vibrance(sample_wide(mix(q, u_center, 0.55), spread_px), 2.4);
      rgb = min(bg * bg * 1.05, vec3(1.0)) * u_shadow.w;
    }
    under = vec4(rgb * a, a);
  }

  vec4 out_c = under;
  if (cov > 0.0) {
    float depth = max(-d, 0.0);
    float op = u_opacity;
    float facing = dot(n, u_light);
    float near = max(facing, 0.0);
    float far = max(-facing, 0.0);

    float even = mix(1.0, -0.55, smoothstep(0.0, 0.5, far));
    float disp = u_optics.x * even * (1.0 - u_corner_disp);
    float corner = u_optics.x * 2.0 * n.x * n.y * u_corner_disp;
    vec2 base = u_center + (p - u_center) / max(u_optics.z, 0.05);
    float side = n.x * n.x * n.x * n.x;
    float shift = lens(depth, side, 0.0);
    float foot = abs(1.0 - lens(depth + 0.5, side, 0.0) + lens(max(depth - 0.5, 0.0), side, 0.0));
    float blur = u_optics.y * mix(0.45, 1.0, smoothstep(0.0, max(u_lens.y, 1.0), depth));

    vec3 c;
    float spread = abs(shift * disp) + abs(corner) * 4.0 * step(depth, max(u_pinch.y, u_lens.y));
    if (blur < 3.0 && (spread > 0.35 || foot > 1.3)) {
      vec3 sum = vec3(0.0);
      vec3 wsum = vec3(0.0);
      float rr = max(blur < 1.2 ? blur : 0.0, min(foot * 0.09, 2.0));
      int steps = abs(corner) > 0.1 ? 16 : 8;
      for (int i = 0; i < 16; i++) {
        if (i >= steps) break;
        float t = (float(i) + 0.5) / float(steps);
        int idx = (i * 5 + 2) - ((i * 5 + 2) / steps) * steps;
        float sub = (float(idx) + 0.5) / float(steps) - 0.5;
        vec3 w = max(vec3(1.0) - abs(vec3(t) - vec3(0.0, 0.5, 1.0)) * 1.9, vec3(0.0));
        float hue = 1.0 - 2.0 * t;
        float k = lens(max(depth + sub, 0.0), side, corner * hue) * (1.0 + disp * hue) - sub;
        sum += sample_bg(base + n * k, rr) * w;
        wsum += w;
      }
      c = sum / max(wsum, vec3(1e-4));
    } else {
      blur = max(blur, min((foot - 1.0) * 0.4, 6.0));
      if (spread > 0.35) {
        c.r = sample_bg(base + n * shift * (1.0 + disp), blur).r;
        c.g = sample_bg(base + n * shift, blur).g;
        c.b = sample_bg(base + n * shift * (1.0 - disp), blur).b;
      } else {
        c = sample_bg(base + n * shift, blur);
      }
    }

    if (u_glow.x > 0.0) {
      float e = u_glow.x * exp(-depth / max(u_lens.y * 0.8, 1.0));
      if (e > 0.008) {
        vec2 q = p + n * (u_lens.y * 0.8 + 6.0 * u_ppp);
        c = mix(c, vibrance(sample_wide(q, 14.0 * u_ppp), 1.5), e);
      }
    }

    c = vibrance(c, mix(1.0, u_optics.w, op));
    float lum = luma(c);
    float lo = mix(0.0, u_pinch.z, op);
    float hi = mix(1.0, u_pinch.w, op);
    c = vec3(lo) + c * (hi - lo);

    float ta = u_tint.a * op;
    if (ta > 0.0) {
      vec3 toned = u_tint.rgb * (0.86 + 0.26 * lum) + vec3(0.05 * lum);
      c = mix(c, mix(u_tint.rgb, toned, u_tint_follow), ta);
    }

    float body = max(min(half_min * 0.95, 34.0 * u_ppp), 1.0);
    float ef = 1.0 - smoothstep(0.0, body, depth);
    c = c * (1.0 - u_glow.z * ef * ef * near * op);
    c = mix(c, vec3(1.0), u_glow.y * ef * ef * far * op);

    float edge = gauss(max(depth - 0.18 * u_ppp, 0.0) / max(0.40 * u_ppp, 0.7));
    if (edge > 0.01 && u_edge.w > 0.0) {
      vec2 q = mix(p - n * half_min, u_center, 0.8);
      vec3 body_c = vibrance(sample_wide(q, max(half_min * 0.85, 6.0 * u_ppp)), 10.0);
      float k = u_edge.w * edge * op * (1.0 - 0.92 * smoothstep(0.1, 0.8, far));
      c = mix(c, body_c * 0.5, k);
    }
    float rw = max(u_edge.y, 0.9);
    float line = gauss((depth - rw * 0.55) / (rw * 0.68));
    float halo = exp(-depth / max(2.0 * u_ppp, 0.8)) * min(u_glow.y * 2.4, 0.6) * smoothstep(0.0, 0.5, far);
    float dir = (1.0 - 0.9 * u_edge.w) * pow(near, 1.6) + pow(far, 1.2);
    float key = u_edge.z + (1.0 - u_edge.z) * min(dir, 1.0);
    vec3 rim_rgb = mix(vec3(1.0), min(vibrance(c, 1.4) * 1.5 + 0.15, vec3(1.0)), 0.25);
    float rim_a = clamp(u_edge.x * (key * line + dir * halo), 0.0, 1.0) * op;
    c = mix(c, rim_rgb, rim_a);

    float fade = smoothstep(0.0, 0.25, op);
    float a = cov * fade;
    out_c = vec4(clamp(c, 0.0, 1.0) * a, a) + under * (1.0 - a);
  }

  if (out_c.a <= 0.001) {
    discard;
  }
  FRAG = out_c;
}
"#;

fn glass_dt(ctx: &egui::Context) -> f32 {
    ctx.input(|i| i.stable_dt).clamp(1.0 / 240.0, 0.05)
}

/// How an analytical damped spring moves (SwiftUI convention).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    pub response: f32,
    pub damping: f32,
}

impl Tuning {
    pub const fn new(response: f32, damping: f32) -> Self {
        Self { response, damping }
    }
    /// Fast without overshoot (colors, opacity, hover).
    pub const SNAP: Self = Self::new(0.20, 1.0);
    /// Smooth position and panel motion.
    pub const SMOOTH: Self = Self::new(0.36, 0.86);
    /// Lifts, presses, and stretches.
    pub const PRESS: Self = Self::new(0.34, 0.64);
    /// Clear liquid wobble for popups and droplets.
    pub const BOUNCY: Self = Self::new(0.46, 0.68);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    started: bool,
}

impl Spring {
    pub fn new(value: f32) -> Self {
        Self { value, velocity: 0.0, started: true }
    }

    pub fn step(&mut self, target: f32, tuning: Tuning, dt: f32) -> bool {
        if !self.started {
            *self = Self::new(target);
            return false;
        }
        let w = std::f32::consts::TAU / tuning.response.max(0.01);
        let z = tuning.damping.clamp(0.05, 1.0);
        let x0 = self.value - target;
        let v0 = self.velocity;
        let (x, v) = if z > 0.999 {
            let c = v0 + w * x0;
            let e = (-w * dt).exp();
            ((x0 + c * dt) * e, (v0 - w * c * dt) * e)
        } else {
            let wd = w * (1.0 - z * z).sqrt();
            let e = (-z * w * dt).exp();
            let (s, c) = (wd * dt).sin_cos();
            let b = (v0 + z * w * x0) / wd;
            (e * (x0 * c + b * s), e * (v0 * c - (z * w * b + wd * x0) * s))
        };
        self.value = target + x;
        self.velocity = v;
        let scale = 1.0 + target.abs() * 0.02;
        if x.abs() < 0.0008 * scale && v.abs() < 0.01 * scale {
            self.value = target;
            self.velocity = 0.0;
            return false;
        }
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spring2d {
    pub x: Spring,
    pub y: Spring,
}

impl Spring2d {
    pub fn value(&self) -> Vec2 {
        vec2(self.x.value, self.y.value)
    }

    pub fn step(&mut self, target: Vec2, tuning: Tuning, dt: f32) -> bool {
        self.x.step(target.x, tuning, dt) | self.y.step(target.y, tuning, dt)
    }
}

pub fn rubber_band(distance: f32, limit: f32) -> f32 {
    let a = distance.abs() * 0.55;
    distance.signum() * (1.0 - 1.0 / (a / limit.max(0.01) + 1.0)) * limit
}

/// Fluid damped-spring animation toward `target`. Requests repaints while settling.
pub fn spring(ctx: &egui::Context, id: egui::Id, target: f32, tuning: Tuning) -> f32 {
    let dt = glass_dt(ctx);
    let mut s = ctx.data(|d| d.get_temp::<Spring>(id)).unwrap_or_default();
    if s.step(target, tuning, dt) {
        ctx.request_repaint();
    }
    ctx.data_mut(|d| d.insert_temp(id, s));
    s.value
}

pub fn spring_bool(ctx: &egui::Context, id: egui::Id, on: bool, tuning: Tuning) -> f32 {
    spring(ctx, id, on as u8 as f32, tuning)
}

#[derive(Clone, Copy, Default)]
struct PressState {
    press: Spring,
    pull: Spring2d,
    hover: Spring,
    origin: Option<egui::Pos2>,
}

/// Fluid press swell, elastic pull toward the pointer, and optical light-bend rotation.
#[derive(Clone, Copy, Debug)]
pub struct PressEffect {
    pub rect: Rect,
    pub scale: f32,
    pub amount: f32,
    pub hover: f32,
    pub bend: f32,
}

impl PressEffect {
    pub fn apply(&self, style: &mut Style) {
        let p = self.amount.max(0.0);
        style.brightness += 0.05 * p;
        style.light_bend += self.bend;
        style.material.refraction *= 1.0 + 0.28 * p;
        style.material.shadow *= 1.0 + 0.30 * p;
    }
}

pub fn press(ui: &egui::Ui, resp: &egui::Response, rest: Rect, strength: f32) -> PressEffect {
    let ctx = ui.ctx();
    let id = resp.id.with("press-fx");
    let mut s = ctx.data(|d| d.get_temp::<PressState>(id)).unwrap_or_default();
    let dt = glass_dt(ctx);
    let held = resp.is_pointer_button_down_on() && strength > 0.0;
    let pointer = ctx.input(|i| i.pointer.interact_pos());
    if held {
        if s.origin.is_none() {
            s.origin = ctx.input(|i| i.pointer.press_origin()).or(pointer);
        }
    } else {
        s.origin = None;
    }
    let mut pull = Vec2::ZERO;
    if held
        && let (Some(p), Some(o)) = (pointer, s.origin)
    {
        let d = p - o;
        let lim = (rest.height() * 0.42).clamp(8.0, 18.0);
        pull = vec2(rubber_band(d.x, lim), rubber_band(d.y, lim));
    }
    let inside = held && resp.hovered();
    let mut moving = s.press.step(
        if held { if inside { 1.0 } else { 0.55 } } else { 0.0 },
        if held { Tuning::new(0.24, 0.78) } else { Tuning::new(0.40, 0.80) },
        dt,
    );
    moving |= s.pull.step(pull, Tuning::new(if held { 0.16 } else { 0.42 }, 0.62), dt);
    moving |= s.hover.step((resp.hovered() && strength > 0.0) as u8 as f32, Tuning::SNAP, dt);
    if moving {
        ctx.request_repaint();
    }
    ctx.data_mut(|d| d.insert_temp(id, s));

    let size = rest.size();
    let grow = (size.y * 0.16).min(7.0) * s.press.value * strength;
    let p = s.pull.value() * strength;
    let stretch = vec2(p.x.abs() * 0.55 - p.y.abs() * 0.22, p.y.abs() * 0.55 - p.x.abs() * 0.22);
    let rect = Rect::from_center_size(
        rest.center() + p * 0.5,
        (size + Vec2::splat(grow) + stretch).max(Vec2::splat(4.0)),
    );
    PressEffect {
        rect: if moving || held { rect } else { rest },
        scale: 1.0 + grow.max(0.0) / size.y.max(1.0) * 0.45,
        amount: s.press.value,
        hover: s.hover.value,
        bend: p.x * 0.018,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Thumb {
    pub pos: Spring,
    pub lift: Spring,
    chase: Spring,
}

#[derive(Clone, Copy, Debug)]
pub struct ThumbFrame {
    pub size: Vec2,
    pub lift: f32,
    pub bend: f32,
}

impl Thumb {
    pub fn step(&mut self, ctx: &egui::Context, target: f32, follow: Tuning, raised: bool, rest: Vec2, lifted: Vec2) -> ThumbFrame {
        let dt = glass_dt(ctx);
        let k = (rest.y / 24.0).clamp(1.0, 3.0);
        let at = 2.4 * k;
        let mut moving = self.pos.step(target, follow, dt);
        let far = (self.pos.value - target).abs() > rest.x * 0.18;
        let up = raised || (far && self.lift.value > 0.3);
        let lt = if up { Tuning::new(0.27, 0.72) } else { Tuning::new(0.36, 0.76) };
        moving |= self.lift.step(up as u8 as f32, lt, dt);
        moving |= self.chase.step(self.pos.velocity, Tuning::new(0.628, 0.5), dt);
        if moving {
            ctx.request_repaint();
        }
        let lift = self.lift.value.max(0.0);
        let u = self.chase.velocity / 1000.0;
        let s = 0.159 * (u / at).tanh();
        let base = lerp_vec2(rest, lifted, lift);
        let w = 1.0 + s * (0.2 + 0.8 * lift.min(1.0));
        ThumbFrame {
            size: vec2(base.x * w, base.y / w),
            lift,
            bend: (self.pos.velocity / 1800.0).clamp(-0.35, 0.35),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Grab {
    off: f32,
    on: bool,
    was_held: bool,
}

pub fn load_thumb(ctx: &egui::Context, id: egui::Id) -> Thumb {
    ctx.data(|d| d.get_temp::<Thumb>(id.with("droplet"))).unwrap_or_default()
}

pub fn store_thumb(ctx: &egui::Context, id: egui::Id, t: Thumb) {
    ctx.data_mut(|d| d.insert_temp(id.with("droplet"), t));
}

pub fn drag_target(ctx: &egui::Context, id: egui::Id, r: &egui::Response, ox: f32, pos: f32, lo: f32, hi: f32, limit: f32) -> Option<f32> {
    let mut g = ctx.data(|d| d.get_temp::<Grab>(id.with("grab"))).unwrap_or_default();
    let held = r.is_pointer_button_down_on() || r.dragged();
    let pointer = ctx.input(|i| i.pointer.interact_pos());
    let mut out = None;
    if held && let Some(p) = pointer {
        if !g.was_held {
            g.off = if (p.x - ox - pos).abs() < limit * 2.5 { p.x - ox - pos } else { 0.0 };
            g.on = true;
        }
        if g.on {
            let x = p.x - ox - g.off;
            out = Some(x.clamp(lo, hi) + rubber_band(x - x.clamp(lo, hi), limit));
        }
    } else {
        g.on = false;
    }
    g.was_held = held;
    ctx.data_mut(|d| d.insert_temp(id.with("grab"), g));
    out
}

pub fn thumb_glass(_rect: Rect, f: &ThumbFrame, knob: Color32, light: bool, scale: f32) -> Style {
    let lens = Material::LENS.scaled(scale);
    let mut rest = lens;
    rest.refraction = 0.0;
    rest.outer_refraction = 0.0;
    rest.dispersion = 0.0;
    rest.blur = 9.0;
    rest.inner_glow = 0.0;
    rest.inner_shadow = 0.0;
    rest.dark_edge = 0.0;
    rest.rim = 1.0;
    rest.rim_base = 0.3;
    rest.rim_width = 0.6 * scale.max(1.0).sqrt();
    rest.light_spill = 0.0;
    rest.shadow = 0.14;
    rest.shadow_radius = 8.0 * scale;
    rest.shadow_y = 2.0 * scale;
    rest.shadow_tint = 0.0;
    let t = f.lift.clamp(0.0, 1.0);
    let mut m = rest.lerp(&lens, t);
    let [r, g, b, _] = knob.to_srgba_unmultiplied();
    let ka = ((1.0 - t * 1.35).max(0.0).powf(1.2) * 255.0) as u8;
    m.tint_follow = 0.0;
    m.blur = lerp(9.0, lens.blur, (t * 1.1).min(1.0));
    Style {
        corner: Corner::Capsule,
        material: m,
        tint: Some(Color32::from_rgba_unmultiplied(r, g, b, ka)),
        darkness: if light { 0.05 } else { 0.90 },
        alpha: 1.0,
        light_bend: f.bend,
        brightness: 0.0,
    }
}

/// Interactive macOS 26/27 Liquid Glass switch: click or drag across `track`.
/// When `row_toggled` is true (e.g. a clickable row flipped `on`), the droplet
/// still performs its refractive lens lift across the track without double-toggling.
pub fn liquid_toggle(
    ui: &mut egui::Ui,
    track: Rect,
    on: &mut bool,
    accent: Color32,
    scale: f32,
    id: egui::Id,
    row_toggled: bool,
) -> egui::Response {
    let light = crate::theme::light();
    let fade = if ui.is_enabled() { 1.0 } else { 0.4 };
    let sense = if row_toggled || !ui.is_enabled() { egui::Sense::hover() } else { egui::Sense::click_and_drag() };
    let mut resp = ui.interact(track, id, sense);
    if ui.is_enabled() {
        resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    }

    let k = scale;
    let rest = vec2(track.height() * 1.36, track.height() - 4.0 * k);
    let lifted = vec2(rest.x * 1.44, rest.y * 1.50);
    let pad = 2.0 * k;
    let ox = track.min.x;
    let x0 = pad + rest.x * 0.5;
    let x1 = track.width() - pad - rest.x * 0.5;
    let mut d = load_thumb(ui.ctx(), id);
    let before = *on;
    let dragged = drag_target(ui.ctx(), id, &resp, ox, d.pos.value, x0, x1, 5.0 * k);
    let mid = (x0 + x1) * 0.5;
    let pointer = ui.ctx().input(|i| i.pointer.interact_pos());
    let origin = ui.ctx().input(|i| i.pointer.press_origin());
    let moved = pointer.is_some_and(|p| origin.is_some_and(|o| (p.x - o.x).abs() > 4.0));
    if resp.drag_stopped() {
        *on = if moved { d.pos.value > mid } else { !before };
        resp.mark_changed();
    } else if resp.clicked() && !row_toggled {
        *on = !before;
        resp.mark_changed();
    }

    let toggled = before != *on || row_toggled;
    let home = if *on { x1 } else { x0 };
    let target = dragged.filter(|_| moved).unwrap_or(home);
    if toggled && d.lift.value < 0.3 {
        d.lift.value = 0.31;
    }
    let follow = if dragged.is_some() && moved { Tuning::new(0.14, 0.9) } else { Tuning::new(0.32, 0.80) };
    let held = resp.is_pointer_button_down_on() || resp.dragged();
    let f = d.step(ui.ctx(), target, follow, held, rest, lifted);
    store_thumb(ui.ctx(), id, d);

    let t = ((d.pos.value - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0);
    let off = if light { Color32::from_black_alpha(38) } else { Color32::from_white_alpha(52) };
    let track_col = crate::theme::with_alpha(crate::theme::lerp_color(off, accent, t), fade);
    ui.painter().rect_filled(track, track.height() * 0.5, track_col);

    let center = egui::pos2(ox + d.pos.value, track.center().y);
    let thumb = Rect::from_center_size(center, f.size);
    if crate::renderer::active() == crate::renderer::Renderer::OpenGl && ui.is_enabled() {
        let style = thumb_glass(thumb, &f, Color32::WHITE, light, k);
        paint(ui, id.with("droplet-glass"), thumb, style);
    } else {
        ui.painter().rect_filled(thumb.translate(vec2(0.0, 1.0)), thumb.height() * 0.5, Color32::from_black_alpha((26.0 * fade) as u8));
        ui.painter().rect_filled(thumb, thumb.height() * 0.5, Color32::from_white_alpha((245.0 * fade) as u8));
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_resolves_continuous_squircle_radii() {
        let (r, p) = Corner::Capsule.resolve(vec2(300.0, 52.0));
        assert!((r.x - 26.0 * 1.16).abs() < 0.01);
        assert!((p - 2.18).abs() < 0.01);

        let (cr, cp) = Corner::Radius(12.0).resolve(vec2(200.0, 120.0));
        assert!((cr.x - 16.2).abs() < 0.01);
        assert!((cp - 2.83).abs() < 0.01);
    }

    #[test]
    fn glsl_shaders_have_valid_uniform_bindings_and_no_wgsl_types() {
        for src in [TENT_VERT, TENT_FRAG, GLASS_VERT, GLASS_FRAG] {
            assert!(!src.contains("vec2f") && !src.contains("vec3f") && !src.contains("vec4f"));
            assert!(!src.contains("select("));
        }
        assert!(GLASS_FRAG.contains("sd_squircle"));
        assert!(GLASS_FRAG.contains("u_mip5"));
        assert!(GLASS_FRAG.contains("u_shadow"));
    }
}
