<div align="center">
  <img src="assets/icon/icon.png" width="128" alt="Dawn FM icon">
  <h1>Dawn FM</h1>
  <p><strong>A fast, lightweight, native YouTube Music player for macOS</strong></p>
  <p>No Electron, no web wrapper. Ultra-light on memory, instant playback, and native design.</p>

  <p>
    <a href="https://github.com/wenujacodes/DawnFM/releases/latest"><img src="https://img.shields.io/github/v/release/wenujacodes/DawnFM?style=flat-square&color=a855f7&labelColor=0d1117" alt="Latest release"></a>
    <a href="https://github.com/wenujacodes/DawnFM/releases"><img src="https://img.shields.io/github/downloads/wenujacodes/DawnFM/total?style=flat-square&color=a855f7&labelColor=0d1117" alt="All-time Downloads"></a>
    <img src="https://img.shields.io/badge/status-active%20development-22c55e?style=flat-square&labelColor=0d1117" alt="Active Development">
    <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows-333?style=flat-square&labelColor=0d1117" alt="Platforms">
    <a href="LICENSE"><img src="https://img.shields.io/github/license/wenujacodes/DawnFM?style=flat-square&color=333&labelColor=0d1117" alt="License: GPL-3.0"></a>
  </p>

  <p>
    <a href="https://github.com/wenujacodes/DawnFM/releases/latest"><b>Download (.dmg)</b></a> ·
    <a href="#features">Features</a> ·
    <a href="#installation">Install</a> ·
    <a href="#building-from-source">Build</a>
  </p>
</div>

---

## Screenshots

<table>
  <tr>
    <td width="50%"><img src="docs/readme/screenshot-1.jpeg" alt="Full player with artwork and lyrics"></td>
    <td width="50%"><img src="docs/readme/screenshot-2.jpeg" alt="Browse view with shelves"></td>
  </tr>
  <tr>
    <td align="center"><b>Player</b> — artwork, synced lyrics & controls</td>
    <td align="center"><b>Browse</b> — music feeds, charts & library</td>
  </tr>
</table>

## Features

- **Blazing Fast & Native**: Pure Rust UI using [egui](https://github.com/emilk/egui) and [rodio](https://github.com/RustAudio/rodio). Smooth 120 FPS animations with ultra-low CPU/RAM usage.
- **Synced Karaoke Lyrics**: Syllable-by-syllable & timed lyrics with dynamic artwork-matched typography.
- **Lossless & Spatial Audio**: Hi-Res Lossless and Dolby Atmos playback with automatic fallback to YouTube audio.
- **Seamless Playback**: DJ-style automix, crossfade, and zero-latency track prefetching.
- **Offline Downloads**: Save tracks and playlists locally for offline listening.
- **Desktop Integrations**: macOS media keys, Now Playing widget, Dock menu, Discord Rich Presence, and Last.fm scrobbling.
- **Library & Jam**: Google sign-in for full library sync and real-time Listen Together jam rooms.

## Privacy

- **Zero Telemetry**: No analytics, tracking scripts, or user profiling.
- **Direct Connections**: All network requests connect directly to YouTube Music and configured sources.
- **Encrypted Storage**: Credentials are securely saved directly in macOS Keychain or Windows Credential Manager.

## Installation

### macOS
1. Download the latest `.dmg` from **[Releases](https://github.com/wenujacodes/DawnFM/releases/latest)**.
2. Open the disk image and drag **Dawn FM** to `/Applications`.
3. **Privacy & Security Unlock**: If macOS blocks the app on first launch, go to **System Settings** → **Privacy & Security**, scroll down to the **Security** section, and click **Open Anyway** (or right-click **Dawn FM** in Applications and choose **Open**).

### Windows
- **Installer**: Download `DawnFM-2.4.3-windows-x64-setup.exe` from **[Releases](https://github.com/wenujacodes/DawnFM/releases/latest)** and run setup.
- **Portable**: Download `DawnFM-2.4.3-windows-x64-portable.zip`, extract to any folder, and run `dawnfm.exe`.
- **SmartScreen Unlock**: If Windows Defender SmartScreen blocks installation or launch, right-click the downloaded `.exe` (or `.zip`) → **Properties** → check **Unblock** at the bottom → click **OK** (or click **More info** → **Run anyway**).

## Building from source

Requires the [Rust toolchain](https://rustup.rs/):

```sh
git clone https://github.com/wenujacodes/DawnFM.git
cd DawnFM
cargo build --release
sh macos/bundle.sh target/release/dawnfm 2.4.3 out
```

The signed bundle will be located at `out/Dawn FM.app`.

## License & Credits

- App icon designed by **[@RamiruWanigathunga](https://github.com/RamiruWanigathunga)**.
- Based on [BitChord-PC-Continued](https://github.com/jackychan4891/BitChord-PC-Continued) by **[jackychan4891](https://github.com/jackychan4891)**.
- Dawn FM is licensed under [GPL-3.0](LICENSE). See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.
