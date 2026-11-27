#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, OnceLock};

use crossbeam_channel::unbounded;
use dawnfm::*;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> eframe::Result {
    // Child-process modes for the webview windows (see login.rs).
    let mut args = std::env::args_os().skip(1);
    if let (Some(flag), Some(dir)) = (args.next(), args.next()) {
        // The parent hands its proxy setting down through the environment.
        proxy::init(std::path::Path::new(""));
        if flag == login::FLAG {
            login::child_main(login::Site::Google, dir.into());
            return Ok(());
        }
        if flag == login::SPOTIFY_FLAG {
            login::child_main(login::Site::Spotify, dir.into());
            return Ok(());
        }
    }
    // A jam link (`ytm-rs://jam/CODE` or the https share link): hand it to the running
    // instance if there is one, else join once this one is up. Before `init_logging`,
    // which would truncate the running instance's log file.
    if let Some(code) = deeplink::from_args() {
        if deeplink::forward_to_running(&code) {
            return Ok(());
        }
        deeplink::push(code);
    } else if deeplink::forward_show() {
        return Ok(());
    }
    migrate_data();
    init_logging();
    i18n::detect_and_set_locale();
    // Jam links on macOS come as an Apple Event, not argv: catch it before the loop starts.
    deeplink::install();
    update::clean_portable();
    stream::init_dev_hooks();
    proxy::init(&data_dir());

    // Dev hook: YTM_BACKGROUND opens the window off-screen, unfocused and off the
    // taskbar, so automated test runs don't pop up over whatever the user is doing.
    let background = std::env::var_os("YTM_BACKGROUND").is_some();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Dawn FM")
        .with_app_id("dawnfm")
        .with_inner_size(DEFAULT_SIZE)
        .with_min_inner_size(MIN_SIZE)
        .with_transparent(true)
        // No grey title bar: the content runs to the top edge, under the window buttons.
        .with_decorations(!ui::custom_titlebar())
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false)
        .with_icon(appicon::get(appicon::load(&data_dir())).icon_data().expect("bundled icons are valid PNGs"));
    if background {
        viewport = viewport.with_position([-4000.0, 0.0]).with_inner_size([1920.0, 1057.0]).with_active(false).with_taskbar(false);
    }
    let renderer = renderer::init(&data_dir());
    let options = eframe::NativeOptions {
        viewport,
        renderer: match renderer {
            renderer::Renderer::OpenGl => eframe::Renderer::Glow,
            renderer::Renderer::Wgpu => eframe::Renderer::Wgpu,
        },
        persist_window: !background,
        // Settings live with the rest of the data: beside the portable exe, or a
        // YTM_DATA_DIR profile (e.g. a clean signed-out one for screenshots).
        persistence_path: Some(data_dir().join("app.ron")),
        // No MSAA/depth/stencil: egui doesn't need them and they cost VRAM.
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        // Dev hook: YTM_FPS paces frames itself (see `dev_pace`), so vsync is off.
        glow_options: eframe::egui_glow::GlowConfiguration { vsync: std::env::var_os("YTM_FPS").is_none(), ..Default::default() },
        wgpu_options: renderer::wgpu_options(),
        ..Default::default()
    };

    let run = eframe::run_native(
        "Dawn FM",
        options,
        Box::new(|cc| {
            let ctx = cc.egui_ctx.clone();
            theme::install_style(&ctx);
            theme::install_fonts(&ctx, if cfg!(target_os = "macos") { "sfpro" } else { "inter" });

            // Background threads wake the UI through this; set once the context exists.
            static CTX: OnceLock<egui::Context> = OnceLock::new();
            let _ = CTX.set(ctx.clone());
            let wake = || {
                if let Some(c) = CTX.get() {
                    c.request_repaint();
                }
            };

            // Jam links (the Apple Event handler in `deeplink`) wake the UI through this.
            deeplink::listen(wake);
            // The Dock icon's menu: winit's delegate is set up by now.
            dock::install(wake);
            // The Mac menu bar (Dawn FM, Controls, View, Window, Help).
            menubar::install(wake);
            // Native macOS window vibrancy (NSVisualEffectView behind content view).
            theme::apply_native_vibrancy();
            theme::align_macos_traffic_lights();
            // Keep `ytm-rs:` links pointing at this copy; dev builds leave it alone
            // (even with YTM_UPDATE_FORCE).
            if update::installed() && !cfg!(debug_assertions) {
                deeplink::register_protocol();
            }

            let volume = cc
                .storage
                .and_then(|s| s.get_string("volume"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.8f32);
            // The slider is logarithmic unless turned off. A volume saved before that
            // existed was linear: move it to where it sounds the same.
            let log_setting = cc.storage.and_then(|s| s.get_string("log_volume"));
            let log_volume = log_setting.as_deref().is_none_or(|v| v == "1");
            let volume = if log_setting.is_none() { volume.cbrt() } else { volume };
            // Dev hook: YTM_MUTE starts silent and never persists the volume.
            let volume = if std::env::var_os("YTM_MUTE").is_some() { 0.0 } else { volume };

            let device = cc.storage.and_then(|s| s.get_string("output_device")).filter(|d| !d.is_empty());

            let (audio_tx, audio_rx) = unbounded();
            let audio = audio::spawn(volume, log_volume, device, audio_tx, wake);

            let data_dir = data_dir();
            let (resp_tx, resp_rx) = unbounded();
            let backend = backend::Backend::new(data_dir, resp_tx, audio.clone(), Arc::new(wake))?;

            egui_extras::install_image_loaders(&ctx);
            let http_images = Arc::new(images::HttpImages::new(backend.clone()));
            images::register_http_images(http_images.clone());
            ctx.add_bytes_loader(http_images);

            let (media_tx, media_rx) = unbounded();
            if let Some(h) = hwnd(cc) {
                appicon::set_hwnd(h);
                fullscreen::set_hwnd(h);
                ui::set_titlebar_hwnd(h);
                // eframe sets only the small icon; the taskbar button wants the big one.
                appicon::set_window_icon(appicon::get(appicon::load(backend.data_dir())));
            }
            let media = media::Media::new(hwnd(cc), media_tx, wake);

            // Which GL driver this is, for reports of drawing problems.
            if let Some(gl) = &cc.gl {
                use eframe::glow::HasContext;
                // SAFETY: plain string queries on the current context.
                let (renderer, version) = unsafe { (gl.get_parameter_string(eframe::glow::RENDERER), gl.get_parameter_string(eframe::glow::VERSION)) };
                log::info!("gl: {renderer} ({version})");
            }
            if let Some(rs) = &cc.wgpu_render_state {
                let info = rs.adapter.get_info();
                log::info!("wgpu: {} ({:?}, driver {} {})", info.name, info.backend, info.driver, info.driver_info);
            }
            // The animated background needs the GPU context; without it the UI falls back
            // to a flat gradient.
            let shader = std::env::var_os("YTM_NO_SHADER").is_none();
            let bg = match (&cc.gl, &cc.wgpu_render_state) {
                (Some(gl), _) if shader => shader::Background::new(gl).map_err(|e| log::warn!("background shader unavailable: {e:#}")).ok(),
                (_, Some(rs)) if shader => Some(shader::Background::new_wgpu(rs)),
                _ => None,
            };

            Ok(Box::new(ui::App::new(
                backend,
                audio,
                ui::Channels { resp_rx, audio_rx, media_rx },
                media,
                bg,
                cc.storage,
            )))
        }),
    );
    // A wgpu window that won't open would fail the same way at every launch: go back
    // to OpenGL for the next one.
    if run.is_err() && renderer == renderer::Renderer::Wgpu {
        log::error!("renderer: {} failed to start; switching back to OpenGL", renderer::api_name());
        let _ = renderer::save(&data_dir(), renderer::Renderer::OpenGl);
    }
    run
}

#[cfg(windows)]
fn hwnd(cc: &eframe::CreationContext) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match cc.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get() as *mut std::ffi::c_void),
        _ => None,
    }
}

#[cfg(not(windows))]
fn hwnd(_: &eframe::CreationContext) -> Option<*mut std::ffi::c_void> {
    None
}

/// Log to stderr in debug builds; release builds have no console, so log to a file
/// next to the settings (overwritten each launch).
fn init_logging() {
    let mut builder = env_logger::Builder::from_env(
        // A test build can be made more talkative: YTM_DEFAULT_LOG=warn,bitchord=debug cargo build.
        env_logger::Env::default().default_filter_or(option_env!("YTM_DEFAULT_LOG").unwrap_or("warn,dawnfm=info")),
    );
    // Dev hook: YTM_LOG=<path> logs there (test runs beside a running copy).
    if let Some(path) = std::env::var_os("YTM_LOG") {
        if let Ok(file) = std::fs::File::create(path) {
            builder.target(env_logger::Target::Pipe(Box::new(file)));
        }
    } else if !cfg!(debug_assertions) {
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);
        // The last run's log too: reopening the app after a problem would otherwise wipe it.
        let _ = std::fs::rename(dir.join("dawnfm.log"), dir.join("dawnfm.prev.log"));
        if let Ok(file) = std::fs::File::create(dir.join("dawnfm.log")) {
            builder.target(env_logger::Target::Pipe(Box::new(file)));
        }
    }
    builder.init();
}


