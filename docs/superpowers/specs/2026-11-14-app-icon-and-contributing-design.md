# Design: App icon refresh + contribution workflow

Date: 2026-11-14  
Repos: `RamiruWanigathunga/DawnFM` (fork of `wenujacodes/DawnFM`)  
Source icon: Icon Composer package at `/Users/macbook/Downloads/dawnfm.icon`

## Goals

1. Replace Dawn FM’s application icon everywhere with the new Icon Composer design.
2. Establish issue → fork branch → upstream PR as the default contribution process, documented in `CONTRIBUTING.md`.
3. Demonstrate that process by shipping both changes as upstream issues closed by PRs from this fork.

## Non-goals

- Upstream CI / GitHub Actions changes
- Developer ID / notarization changes
- Redesigning the marketing site layout beyond swapping icon / OG image assets
- Changing in-app UI glyphs under `assets/icon/glyphs/`

## Contribution workflow

For every change (feature, bug, chore):

1. Open an issue on **`wenujacodes/DawnFM`**.
2. Create a feature branch on **`RamiruWanigathunga/DawnFM`**.
3. Commit and push on the fork.
4. Open a PR into **`wenujacodes/DawnFM`** with `Closes #<issue>` in the body.
5. Keep fork `main` synced with upstream after merges.

`CONTRIBUTING.md` at the repo root documents this loop. `README.md` gains a short “Contributing” section that links to it.

## Icon architecture

### Source of truth

Commit the Icon Composer package as:

```
assets/icon/dawnfm.icon/
  icon.json
  Assets/
    Sixteenth_note_with_upwards_stem.svg
    Image 2.png
```

### Derived rasters

Generate a master **1024×1024** PNG from the Composer design, then derive:

| Path | Role |
|------|------|
| `assets/icon/icon.png` | Window / tray / README / fallback |
| `assets/icon/icon.ico` | Embedded / Windows-style shortcut icon |
| `assets/icon/AppIcon.iconset/*` | Pre–macOS 26 Dock/Finder `.icns` |
| `assets/icon/AppIcon_macos26.iconset/*` | macOS 26+ `.icns` fallback |
| `AppIcons/Assets.xcassets/AppIcon.appiconset/*` | Xcode-style appiconset |
| `AppIcons/appstore.png`, `AppIcons/playstore.png` | Store marketing sizes |
| `site/favicon-32.png`, `site/apple-touch-icon.png`, `site/icon-192.png`, `site/icon-512.png` | Site icons |
| `site/og.png` | Social card: crop/place master icon on a simple branded canvas (no full site redesign) |

### Bundle wiring (`macos/bundle.sh`)

- When `assets/icon/dawnfm.icon` exists, copy it into `Contents/Resources/` for macOS 26+ so Finder/Dock can use the Composer icon.
- Keep generating `dawnfm.icns` / `dawnfm-26.icns` from the iconsets so older macOS and current tooling keep working.
- Rust paths already load `assets/icon/icon.png` / `icon.ico` via `include_bytes!`; no Rust changes unless rebuild proves otherwise.

## Delivery (two upstream issues → two PRs)

| Upstream issue | Fork branch | PR closes | Contents |
|----------------|-------------|-----------|----------|
| Feature: Refresh Dawn FM app icon | `feat/app-icon` | that issue | `.icon` source, all derived assets, `bundle.sh` updates |
| Chore: Add CONTRIBUTING.md | `docs/contributing` | that issue | `CONTRIBUTING.md` + README link |

Order: create both issues first → icon PR → CONTRIBUTING PR (docs may cite the icon PR as an example).

Commits live on the fork; issues and PRs target `wenujacodes/DawnFM`.

## Verification

- After `cargo build --release` + `macos/bundle.sh`, Dock/Finder show the new icon.
- App window and tray show the new icon.
- README and site favicon/icon images show the new art.
- Each PR body includes `Closes #<n>` and links the matching upstream issue.

## Risks / notes

- Icon Composer `.icon` is not a classic `.iconset`; `iconutil` alone cannot compile it. Bundle both the `.icon` package and pre-rendered `.icns`/PNGs.
- Master raster export may require Icon Composer / Quick Look / `actool`; if automated export fails, export 1024 PNG once from Icon Composer and generate all sizes with `sips`/`iconutil`.
