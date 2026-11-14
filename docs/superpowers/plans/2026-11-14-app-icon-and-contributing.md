# App Icon Refresh + CONTRIBUTING Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the new Icon Composer app icon everywhere and add CONTRIBUTING.md, each via an upstream issue closed by a PR from this fork.

**Architecture:** Keep `assets/icon/dawnfm.icon` as the Composer source of truth; export one 1024×1024 master PNG; derive all PNG/ICO/iconset/site/store assets from it; update `macos/bundle.sh` to copy the `.icon` into the app bundle on macOS 26+. Document the issue → fork branch → upstream PR loop in `CONTRIBUTING.md`.

**Tech Stack:** macOS `sips` / `iconutil` / `qlmanage`, Python 3 + Pillow for ICO/OG, GitHub (`gh`) for issues/PRs, shell `macos/bundle.sh`, Rust tests in `src/appicon.rs`.

**Spec:** `docs/superpowers/specs/2026-11-14-app-icon-and-contributing-design.md`

## Global Constraints

- Commits and branches live on fork `RamiruWanigathunga/DawnFM`.
- Issues and PRs target upstream `wenujacodes/DawnFM`.
- Every PR body must include `Closes #<n>` for its upstream issue.
- Do not modify `assets/icon/glyphs/` or optional icon variants under `assets/icon/variants/` (only the classic `icon.png` / `icon.ico` defaults).
- Do not change CI, code signing, or site layout beyond icon/OG asset swaps.
- Work from isolated feature branches (`feat/app-icon`, `docs/contributing`); do not commit unrelated files.

## File map

| Path | Responsibility |
|------|----------------|
| `assets/icon/dawnfm.icon/` | Icon Composer source (new) |
| `assets/icon/icon.png` | Master flat 1024 PNG + window/tray/README source |
| `assets/icon/icon.ico` | Multi-size ICO for classic variant |
| `assets/icon/AppIcon.iconset/` | Pre-26 Dock/Finder iconset |
| `assets/icon/AppIcon_macos26.iconset/` | 26+ icns fallback iconset |
| `AppIcons/.../AppIcon.appiconset/` + store PNGs | Xcode/store marketing |
| `site/favicon-32.png`, `apple-touch-icon.png`, `icon-192.png`, `icon-512.png`, `og.png` | Site / social |
| `macos/bundle.sh` | Copy `.icon` + keep icns generation |
| `CONTRIBUTING.md` | Contribution workflow (new) |
| `README.md` | Link to CONTRIBUTING |

---

### Task 1: Open upstream issues

**Files:**
- None (GitHub only)

**Interfaces:**
- Consumes: none
- Produces: upstream issue numbers `ICON_ISSUE` and `DOCS_ISSUE` (record in the plan checklist comments / shell env for later tasks)

- [ ] **Step 1: Create the icon issue on upstream**

```bash
gh issue create --repo wenujacodes/DawnFM \
  --title "Feature: Refresh Dawn FM app icon" \
  --body "$(cat <<'EOF'
## Summary
Replace the current Dawn FM application icon with a new Icon Composer design across Dock/Finder, window/tray, README, site favicons, and store marketing PNGs.

## Motivation
The fork has a finished Icon Composer (`.icon`) design that should become the canonical app identity.

## Acceptance criteria
- [ ] Icon Composer source checked into the repo
- [ ] Dock/Finder icon updated (including macOS 26+ `.icon` path where applicable)
- [ ] Window/tray / classic `icon.png` + `icon.ico` updated
- [ ] README + site favicons/icons + App Store / Play Store PNGs updated
- [ ] Bundle script wires the new assets

EOF
)"
```

Expected: prints a URL like `https://github.com/wenujacodes/DawnFM/issues/N`. Save `N` as `ICON_ISSUE`.

- [ ] **Step 2: Create the CONTRIBUTING issue on upstream**

```bash
gh issue create --repo wenujacodes/DawnFM \
  --title "Chore: Add CONTRIBUTING.md (issue → PR workflow)" \
  --body "$(cat <<'EOF'
## Summary
Document the default contribution process: open an issue on this repo, implement on a fork branch, open a PR that closes the issue.

## Acceptance criteria
- [ ] `CONTRIBUTING.md` at repo root describing the loop
- [ ] README links to it under a short Contributing section

EOF
)"
```

Expected: prints URL with issue number. Save as `DOCS_ISSUE`.

- [ ] **Step 3: Record both numbers**

```bash
echo "ICON_ISSUE=<n>"
echo "DOCS_ISSUE=<n>"
```

No commit (nothing local yet).

---

### Task 2: Branch + commit Icon Composer source

**Files:**
- Create: `assets/icon/dawnfm.icon/icon.json`
- Create: `assets/icon/dawnfm.icon/Assets/Sixteenth_note_with_upwards_stem.svg`
- Create: `assets/icon/dawnfm.icon/Assets/Image 2.png`

**Interfaces:**
- Consumes: `/Users/macbook/Downloads/dawnfm.icon`
- Produces: tracked `assets/icon/dawnfm.icon/` tree for later bundle copy

- [ ] **Step 1: Create branch from up-to-date main**

```bash
cd /Users/macbook/Documents/Code/Shared/Wenuja/DawnFM
git checkout main
git pull origin main
git checkout -b feat/app-icon
```

Expected: on `feat/app-icon`.

- [ ] **Step 2: Copy the Icon Composer package into the repo**

```bash
rm -rf assets/icon/dawnfm.icon
cp -R /Users/macbook/Downloads/dawnfm.icon assets/icon/dawnfm.icon
# Ensure no AppleDouble junk
find assets/icon/dawnfm.icon -name '._*' -delete
ls -la assets/icon/dawnfm.icon assets/icon/dawnfm.icon/Assets
```

Expected: `icon.json` + `Assets/` with SVG and PNG.

- [ ] **Step 3: Commit source only**

```bash
git add assets/icon/dawnfm.icon
git commit -m "$(cat <<'EOF'
Add Icon Composer dawnfm.icon as app icon source.

EOF
)"
```

---

### Task 3: Export master 1024 PNG

**Files:**
- Create (temp then promote): `assets/icon/icon-master-1024.png` (optional temp) → overwrite `assets/icon/icon.png`

**Interfaces:**
- Consumes: `assets/icon/dawnfm.icon/`
- Produces: `assets/icon/icon.png` at exactly 1024×1024 RGBA

- [ ] **Step 1: Try Quick Look thumbnail export of the `.icon`**

```bash
tmp=$(mktemp -d)
qlmanage -t -s 1024 -o "$tmp" assets/icon/dawnfm.icon 2>&1
ls -la "$tmp"
file "$tmp"/*
```

If a usable PNG appears (≈1024×1024), copy it:

```bash
cp "$tmp"/*.png /tmp/dawnfm-master-1024.png
sips -z 1024 1024 /tmp/dawnfm-master-1024.png --out assets/icon/icon.png
file assets/icon/icon.png
```

Expected: `PNG image data, 1024 x 1024`.

- [ ] **Step 2: Fallback if qlmanage fails — export from Icon Composer**

Open the package and export a 1024 PNG manually:

```bash
open -a "Icon Composer" assets/icon/dawnfm.icon
```

Save/export as `/tmp/dawnfm-master-1024.png`, then:

```bash
sips -z 1024 1024 /tmp/dawnfm-master-1024.png --out assets/icon/icon.png
file assets/icon/icon.png
```

Do **not** hand-composite the SVG unless both automated and Icon Composer export fail; if that happens, stop and ask Ramiru for a 1024 export rather than guessing the liquid-glass look.

- [ ] **Step 3: Visual sanity check**

```bash
open assets/icon/icon.png
```

Confirm pink/red gradient + sixteenth-note mark (not the old icon).

- [ ] **Step 4: Commit master PNG**

```bash
git add assets/icon/icon.png
git commit -m "$(cat <<'EOF'
Update master icon.png from Icon Composer design.

EOF
)"
```

---

### Task 4: Regenerate iconsets, AppIcons, ICO, and site assets

**Files:**
- Modify: `assets/icon/AppIcon.iconset/icon_*.png`
- Modify: `assets/icon/AppIcon_macos26.iconset/icon_*.png`
- Modify: `AppIcons/Assets.xcassets/AppIcon.appiconset/{16,32,64,128,256,512,1024}.png`
- Modify: `AppIcons/appstore.png`, `AppIcons/playstore.png`
- Modify: `assets/icon/icon.ico`
- Modify: `site/favicon-32.png`, `site/apple-touch-icon.png`, `site/icon-192.png`, `site/icon-512.png`, `site/og.png`

**Interfaces:**
- Consumes: `assets/icon/icon.png` (1024×1024)
- Produces: all derived rasters listed above; classic ICO must include sizes 16, 32, and 256 (required by `src/appicon.rs` tests)

- [ ] **Step 1: Write a one-shot generator script (run, do not commit the script unless useful long-term — prefer not committing)**

Run this inline with Python (Pillow already available):

```bash
python3 <<'PY'
from pathlib import Path
from PIL import Image

root = Path("/Users/macbook/Documents/Code/Shared/Wenuja/DawnFM")
master = Image.open(root / "assets/icon/icon.png").convert("RGBA")
assert master.size == (1024, 1024), master.size

def save_resized(img, path, size):
    out = img.resize((size, size), Image.Resampling.LANCZOS)
    path.parent.mkdir(parents=True, exist_ok=True)
    out.save(path, "PNG")
    print("wrote", path, size)

# macOS iconsets (same art for both sets per design)
for set_name in ("AppIcon.iconset", "AppIcon_macos26.iconset"):
    base = root / "assets/icon" / set_name
    mapping = {
        "icon_16x16.png": 16,
        "icon_16x16@2x.png": 32,
        "icon_32x32.png": 32,
        "icon_32x32@2x.png": 64,
        "icon_128x128.png": 128,
        "icon_128x128@2x.png": 256,
        "icon_256x256.png": 256,
        "icon_256x256@2x.png": 512,
        "icon_512x512.png": 512,
        "icon_512x512@2x.png": 1024,
    }
    for name, size in mapping.items():
        save_resized(master, base / name, size)

# AppIcons appiconset
appiconset = root / "AppIcons/Assets.xcassets/AppIcon.appiconset"
for size in (16, 32, 64, 128, 256, 512, 1024):
    save_resized(master, appiconset / f"{size}.png", size)

# Store marketing (keep ~same pixel sizes as existing files)
save_resized(master, root / "AppIcons/appstore.png", 1024)
save_resized(master, root / "AppIcons/playstore.png", 512)

# Site
save_resized(master, root / "site/favicon-32.png", 32)
save_resized(master, root / "site/apple-touch-icon.png", 180)
save_resized(master, root / "site/icon-192.png", 192)
save_resized(master, root / "site/icon-512.png", 512)

# OG: 1200x630, centered icon on icon gradient approx #EB345C -> #EA343A
og = Image.new("RGB", (1200, 630), (234, 52, 90))
icon = master.resize((420, 420), Image.Resampling.LANCZOS)
og.paste(icon, ((1200 - 420) // 2, (630 - 420) // 2), icon)
og.save(root / "site/og.png", "PNG")
print("wrote site/og.png")

# ICO with 16, 32, 64, 128, 256
sizes = [16, 32, 64, 128, 256]
ico_imgs = [master.resize((s, s), Image.Resampling.LANCZOS) for s in sizes]
ico_imgs[0].save(
    root / "assets/icon/icon.ico",
    format="ICO",
    sizes=[(s, s) for s in sizes],
    append_images=ico_imgs[1:],
)
print("wrote assets/icon/icon.ico")
PY
```

Expected: script prints `wrote ...` for each path with no assertion errors.

- [ ] **Step 2: Verify key dimensions**

```bash
file assets/icon/icon.png assets/icon/icon.ico \
  assets/icon/AppIcon.iconset/icon_512x512@2x.png \
  AppIcons/Assets.xcassets/AppIcon.appiconset/1024.png \
  site/favicon-32.png site/og.png
```

Expected: PNG sizes match targets; ICO present; `og.png` is 1200×630.

- [ ] **Step 3: Run Rust icon tests**

```bash
cargo test --lib appicon -- --nocapture
```

Expected: `every_icon_has_every_size` and `picks_the_smallest_entry_that_fits` PASS (classic ICO now has 16/32/256).

- [ ] **Step 4: Commit derived assets**

```bash
git add \
  assets/icon/AppIcon.iconset \
  assets/icon/AppIcon_macos26.iconset \
  assets/icon/icon.ico \
  AppIcons \
  site/favicon-32.png site/apple-touch-icon.png \
  site/icon-192.png site/icon-512.png site/og.png
git commit -m "$(cat <<'EOF'
Regenerate iconsets, store, site, and ICO from new master.

EOF
)"
```

---

### Task 5: Wire `.icon` into `macos/bundle.sh`

**Files:**
- Modify: `macos/bundle.sh`

**Interfaces:**
- Consumes: `assets/icon/dawnfm.icon/` directory; existing iconset paths
- Produces: app bundle with `Contents/Resources/dawnfm.icon` (copied package) on macOS 26+, plus existing `dawnfm.icns` / `dawnfm-26.icns`

- [ ] **Step 1: Add composer icon path + copy after Resources mkdir / before signing**

In `macos/bundle.sh`, after the existing iconset variables (around lines 15–18), add:

```sh
composer_icon="$here/../assets/icon/dawnfm.icon"
```

After the existing icns generation block (after the big `if/else` that ends before the portable copy, ~line 78), and **before** the portable/`codesign` section, insert:

```sh
# Icon Composer package for macOS 26+ (liquid glass). Keep .icns for older systems.
if [ -d "$composer_icon" ]; then
    rm -rf "$app/Contents/Resources/dawnfm.icon"
    cp -R "$composer_icon" "$app/Contents/Resources/dawnfm.icon"
fi
```

Do not remove the existing `iconutil` / AppIcons fallback logic.

- [ ] **Step 2: Build and bundle smoke test**

```bash
cargo build --release
ver=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
sh macos/bundle.sh target/release/dawnfm "$ver" out
ls -la "out/Dawn FM.app/Contents/Resources/"
```

Expected: `dawnfm.icns` present; `dawnfm.icon/` directory present; script exits 0.

- [ ] **Step 3: Visual Dock/Finder check**

```bash
open -R "out/Dawn FM.app"
```

Confirm Finder shows the new icon (may need a moment / relaunch Dock if cached). Optional:

```bash
open "out/Dawn FM.app"
```

Confirm window/tray use the new art after launch.

- [ ] **Step 4: Commit bundle wiring**

```bash
git add macos/bundle.sh
git commit -m "$(cat <<'EOF'
Bundle Icon Composer dawnfm.icon into the macOS app resources.

EOF
)"
```

---

### Task 6: Push icon branch and open upstream PR

**Files:**
- None (GitHub)

**Interfaces:**
- Consumes: `ICON_ISSUE` from Task 1; commits on `feat/app-icon`
- Produces: PR URL on `wenujacodes/DawnFM`

- [ ] **Step 1: Push fork branch**

```bash
git push -u origin feat/app-icon
```

- [ ] **Step 2: Open PR to upstream closing the icon issue**

```bash
gh pr create --repo wenujacodes/DawnFM \
  --head RamiruWanigathunga:feat/app-icon \
  --base main \
  --title "Refresh Dawn FM app icon" \
  --body "$(cat <<EOF
## Summary
- Adds Icon Composer \`dawnfm.icon\` as the source of truth
- Regenerates Dock/Finder iconsets, classic PNG/ICO, AppIcons, site favicons, and OG image
- Updates \`macos/bundle.sh\` to copy the Composer icon into the app bundle

Closes #${ICON_ISSUE}

## Test plan
- [ ] \`cargo test --lib appicon\`
- [ ] \`cargo build --release && sh macos/bundle.sh ...\`
- [ ] Confirm Dock/Finder + window/tray show the new icon
- [ ] Spot-check README/site favicon assets

EOF
)"
```

Expected: PR URL printed; PR links/closes the upstream issue.

---

### Task 7: CONTRIBUTING.md + README link

**Files:**
- Create: `CONTRIBUTING.md`
- Modify: `README.md` (add Contributing section before License)

**Interfaces:**
- Consumes: `DOCS_ISSUE` from Task 1; workflow from design spec
- Produces: docs that match how Tasks 1–6 were done

- [ ] **Step 1: Branch from main (include design-spec commit already on main; do not include unfinished icon work)**

```bash
git checkout main
git pull origin main
git checkout -b docs/contributing
```

If `feat/app-icon` is not merged yet, that is fine — CONTRIBUTING does not depend on those files.

- [ ] **Step 2: Create `CONTRIBUTING.md`**

Write this exact content:

```markdown
# Contributing to Dawn FM

Thanks for helping improve Dawn FM.

## Workflow

We use **issues first**, then pull requests that close them.

1. **Open an issue** on [wenujacodes/DawnFM](https://github.com/wenujacodes/DawnFM/issues) describing the bug, feature, or chore.
2. **Fork** the repo (or use your existing fork) and create a branch named for the change (`feat/...`, `fix/...`, `docs/...`).
3. **Implement** on that branch with focused commits.
4. **Open a pull request** into `wenujacodes/DawnFM` from your fork.
5. In the PR body, include `Closes #<issue-number>` so the issue is linked and closed on merge.

## Guidelines

- Keep PRs focused on one issue.
- Prefer small, reviewable commits.
- Match existing code style and project structure.
- For macOS bundle or icon changes, verify with `macos/bundle.sh` after a release build.
- Do not commit secrets, signing keys, or local-only paths.

## Development

See the README for build and packaging steps:

```sh
cargo build --release
sh macos/bundle.sh target/release/dawnfm <version> out
```
```

- [ ] **Step 3: Add README Contributing section**

Insert before `## License & Credits`:

```markdown
## Contributing

Please open an issue first, then a PR that closes it. See [CONTRIBUTING.md](CONTRIBUTING.md).
```

- [ ] **Step 4: Commit**

```bash
git add CONTRIBUTING.md README.md
git commit -m "$(cat <<'EOF'
Add CONTRIBUTING.md and link it from the README.

EOF
)"
```

- [ ] **Step 5: Push and open upstream PR**

```bash
git push -u origin docs/contributing
gh pr create --repo wenujacodes/DawnFM \
  --head RamiruWanigathunga:docs/contributing \
  --base main \
  --title "Add CONTRIBUTING.md for issue → PR workflow" \
  --body "$(cat <<EOF
## Summary
- Adds \`CONTRIBUTING.md\` documenting issue → fork branch → PR with \`Closes #N\`
- Links it from the README

Closes #${DOCS_ISSUE}

## Test plan
- [ ] Read CONTRIBUTING.md for clarity
- [ ] Confirm README link resolves

EOF
)"
```

Expected: second PR URL; closes docs issue.

---

### Task 8: Sync fork remotes (optional hygiene)

**Files:**
- None

- [ ] **Step 1: Ensure upstream remote exists**

```bash
git remote -v
git remote add upstream https://github.com/wenujacodes/DawnFM.git 2>/dev/null || true
git fetch upstream
```

- [ ] **Step 2: After PRs merge (or whenever), sync fork main**

```bash
git checkout main
git pull upstream main
git push origin main
```

Only run the push after upstream has merged, or when intentionally publishing local main commits (e.g. the design spec) to the fork.

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| Issues on upstream | Task 1 |
| Commits on fork | Tasks 2–5, 7 |
| PRs to upstream with `Closes #N` | Tasks 6, 7 |
| `assets/icon/dawnfm.icon` source | Task 2 |
| Derived PNG/iconset/AppIcons/site/OG/ICO | Tasks 3–4 |
| `bundle.sh` copies `.icon` + keeps icns | Task 5 |
| CONTRIBUTING + README link | Task 7 |
| Verify tests + bundle + visuals | Tasks 4–5 |
| No glyphs/variants/CI/signing scope creep | Global Constraints |

## Plan self-review notes

- No TBD/TODO placeholders remain.
- Two independent deliverables are sequenced as separate branches/PRs per the approved design (Approach 1).
- Classic ICO size contract matches `src/appicon.rs` tests (16/32/256).
