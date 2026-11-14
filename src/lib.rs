pub mod appicon;
pub mod apple;
pub mod art;
pub mod artist_signature;
pub mod audio;
pub mod backend;
pub mod bidi;
pub mod dash;
pub mod deeplink;
pub mod discord;
pub mod dock;
pub mod downloads;
pub mod fullscreen;
pub mod hires;
pub mod images;
pub mod innertube;
pub mod jam;
pub mod jsc;
pub mod lastfm;
pub mod login;
pub mod lyrics;
pub mod media;
pub mod menubar;
pub mod model;
pub mod motion;
pub mod notify;
pub mod opus;
pub mod preset;
pub mod proxy;
pub mod renderer;
pub mod shader;
pub mod sponsorblock;
pub mod stream;
pub mod sync;
pub mod theme;
pub mod transfer;
pub mod translate;
pub mod tray;
pub mod turnstile;
pub mod ui;
pub mod update;
pub mod vault;
pub mod webmusic;
pub mod ytdlp;
pub mod ytm;
pub mod ytmapi;

/// Window size on first launch, and the smallest it may be (points).
pub const DEFAULT_SIZE: [f32; 2] = [1200.0, 780.0];
pub const MIN_SIZE: [f32; 2] = [760.0, 480.0];

/// The app's folder in Application Support, and the one it had before the rename.
pub const DATA_NAME: &str = "DawnFM";
/// Earlier names of that folder.
pub const OLD_DATA_NAMES: [&str; 1] = ["ytm-rs"];

/// Settings, caches and the sign-in: `data` beside the portable build's exe, else the
/// user's app data. Dev hook: YTM_DATA_DIR runs on a separate profile.
pub fn data_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("YTM_DATA_DIR") {
        return d.into();
    }
    if let Some(dir) = update::portable_dir() {
        return dir.join("data");
    }
    directories::ProjectDirs::from("", "", DATA_NAME).expect("no home directory").data_dir().to_path_buf()
}

/// First launch after the rename: copy the old folder's settings and caches into the
/// new one (a copy, so another app still using the old folder keeps it).
pub fn migrate_data() {
    if std::env::var_os("YTM_DATA_DIR").is_some() || update::portable_dir().is_some() {
        return;
    }
    let Some(new) = directories::ProjectDirs::from("", "", DATA_NAME) else { return };
    let new = new.data_dir();
    // The newest earlier folder that exists.
    let Some(old) = OLD_DATA_NAMES.iter().filter_map(|n| directories::ProjectDirs::from("", "", n)).map(|d| d.data_dir().to_path_buf()).find(|d| d.is_dir()) else { return };
    let old = old.as_path();
    if new.exists() {
        return;
    }
    fn copy(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)?.flatten() {
            let name = e.file_name();
            // Old logs and half-done update downloads aren't worth keeping.
            if name.to_string_lossy().ends_with(".log") || name == "updates" {
                continue;
            }
            let (src, dst) = (e.path(), to.join(&name));
            if e.file_type()?.is_dir() { copy(&src, &dst)? } else { std::fs::copy(&src, &dst).map(|_| ())? }
        }
        Ok(())
    }
    // Into a temporary folder first, so a failed copy is tried again next launch.
    let tmp = new.with_extension("moving");
    let _ = std::fs::remove_dir_all(&tmp);
    if copy(old, &tmp).and_then(|_| std::fs::rename(&tmp, new)).is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
