<div align="center">
  <img src="assets/icon/icon.png" width="120" alt="Dawn FM">
  <h1>Dawn FM</h1>
  <p><strong>Fast, lightweight native YouTube Music player for macOS & Windows</strong></p>
  <p>Pure Rust · No Electron · 120 FPS animations · Ultra-low memory</p>

  <p>
    <a href="https://github.com/wenujacodes/DawnFM/releases/latest"><img src="https://img.shields.io/github/v/release/wenujacodes/DawnFM?style=flat-square&color=a855f7&labelColor=0d1117" alt="Latest release"></a>
    <a href="https://github.com/wenujacodes/DawnFM/releases"><img src="https://img.shields.io/github/downloads/wenujacodes/DawnFM/total?style=flat-square&color=a855f7&labelColor=0d1117" alt="All-time Downloads"></a>
    <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows-333?style=flat-square&labelColor=0d1117" alt="macOS | Windows">
    <a href="LICENSE"><img src="https://img.shields.io/github/license/wenujacodes/DawnFM?style=flat-square&color=333&labelColor=0d1117" alt="License: GPL-3.0"></a>
  </p>

  <p>
    <a href="https://github.com/wenujacodes/DawnFM/releases/latest"><b>Download Releases</b></a> ·
    <a href="#key-features">Features</a> ·
    <a href="#installation">Install</a> ·
    <a href="#build-from-source">Build</a>
  </p>
</div>

---

<table>
  <tr>
    <td width="50%"><img src="docs/readme/screenshot-1.jpeg" alt="Player view"></td>
    <td width="50%"><img src="docs/readme/screenshot-2.jpeg" alt="Browse view"></td>
  </tr>
</table>

## Key Features

- **Native & Ultra-Fast**: Built with Rust, [egui](https://github.com/emilk/egui), and [rodio](https://github.com/RustAudio/rodio). Instant playback, near-zero idle resource usage, and smooth 120 FPS UI.
- **Cross-Platform**: First-class support for **macOS** (Apple Silicon & Intel) and **Windows** (frameless custom titlebar, installer & portable zip).
- **Karaoke Lyrics**: Timed syllable-by-syllable synchronized lyrics with artwork-adaptive blur and typography.
- **Hi-Res Audio**: Lossless & Dolby Atmos playback with DJ automix and seamless crossfade.
- **Offline & Full Screen**: Local song downloads, library sync via Google sign-in, and immersive auto-hiding full screen mode.
- **Privacy First**: Zero telemetry, no analytics, direct YouTube Music connection, and credentials encrypted in OS keychain/credential manager.

## Installation

Download the latest version from **[GitHub Releases](https://github.com/wenujacodes/DawnFM/releases/latest)**:

| Platform | Download | Note |
|---|---|---|
| **macOS** | `.dmg` (ARM64 / x64) | Drag to Applications. If prompted, click *Open Anyway* in System Settings. |
| **Windows** | Setup `.exe` or Portable `.zip` | If SmartScreen appears, click *More info* → *Run anyway* (or file Properties → Unblock). |

## Build from Source

```sh
git clone https://github.com/wenujacodes/DawnFM.git && cd DawnFM
cargo build --release
sh macos/bundle.sh target/release/dawnfm 2.4.4 out
```

## Credits

- App icon: **[@RamiruWanigathunga](https://github.com/RamiruWanigathunga)**
- Upstream base: [BitChord-PC-Continued](https://github.com/jackychan4891/BitChord-PC-Continued) by **[jackychan4891](https://github.com/jackychan4891)**
- Licensed under [GPL-3.0](LICENSE)
