# pixa 🖼️

<p align="center">
  <img src="docs/images/banner.webp" alt="pixa — fast image processing CLI" width="800">
</p>

<p align="center">
  <a href="https://crates.io/crates/pixa"><img src="https://img.shields.io/crates/v/pixa.svg" alt="Crates.io"></a>
  <a href="https://crates.io/crates/pixa"><img src="https://img.shields.io/crates/d/pixa.svg" alt="Downloads"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
  <a href="https://github.com/techarm/pixa/actions/workflows/release.yml"><img src="https://github.com/techarm/pixa/actions/workflows/release.yml/badge.svg" alt="CI"></a>
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="README.ja.md">日本語</a>
</p>

A fast Rust image processing CLI — optimize AI-generated images for the
web in one command, split sprite/expression sheets into individual
avatars, generate favicon sets from a logo, remove Gemini AI
watermarks, and more.

## Features

| Command            | Description                                                          |
| ------------------ | -------------------------------------------------------------------- |
| `compress`         | Compress, resize, and convert formats (MozJPEG / OxiPNG / WebP)      |
| `convert`          | Convert between JPEG ↔ PNG ↔ WebP ↔ BMP ↔ GIF ↔ TIFF                 |
| `info`             | Show dimensions, color, EXIF, SHA-256, and other metadata            |
| `favicon`          | Generate a web-ready icon set (ICO + PNGs) from any image            |
| `split`            | Auto-detect and crop individual objects from a sheet image           |
| `transparent`      | Key out a solid background color (chroma-key) to produce an RGBA PNG |
| `remove-watermark` | Remove the Gemini AI watermark via Reverse Alpha Blending            |
| `detect`           | Score whether a Gemini watermark is present in an image              |
| `install`          | Install the Claude Code skill so coding agents can use pixa          |

`compress`, `convert`, `transparent`, and `remove-watermark` accept
either a file or a directory — pass `-r/--recursive` to walk into
subdirectories.

> The watermark removal algorithm is adapted from
> [GeminiWatermarkTool](https://github.com/allenk/GeminiWatermarkTool)
> by Allen Kuo (MIT License).

## Installation

### Homebrew (macOS / Linux)

```bash
brew tap techarm/tap
brew install pixa
```

Subsequent updates: `brew upgrade pixa`.

### Prebuilt binary installer — no Rust toolchain required

**macOS / Linux:**

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/techarm/pixa/releases/latest/download/pixa-installer.sh | sh
```

**Windows (PowerShell):**

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/techarm/pixa/releases/latest/download/pixa-installer.ps1 | iex"
```

The installer downloads the right binary for your platform, places it
on your `$PATH`, and prints the install location. Supported platforms:
macOS (Intel + Apple Silicon), Linux (x86_64 + ARM64), Windows (x86_64).

You can also grab the archive directly from the
[Releases page](https://github.com/techarm/pixa/releases/latest).

### From crates.io

```bash
cargo install pixa
```

Builds the latest published version from source. Requires a Rust
toolchain plus the system dependencies for `mozjpeg` (see below).

### From source

Requirements:

- Rust 1.87+
- CMake, NASM, pkg-config (needed by `mozjpeg`)

```bash
# Ubuntu / Debian
sudo apt install cmake nasm pkg-config libclang-dev

# macOS
brew install cmake nasm pkg-config

# Build
git clone https://github.com/techarm/pixa
cd pixa
cargo build --release
```

The binary lands at `target/release/pixa`. Put it on your `$PATH`:

```bash
cp target/release/pixa ~/.local/bin/   # or anywhere on PATH
```

## Setup (one-time)

After installing pixa, run this once to enable shell tab-completion
and AI coding agent integration:

```bash
pixa install --skills --completions
```

This installs:
- **Shell completions** — auto-detected (zsh/bash/fish) and placed in
  the standard directory (same as `brew`-installed tools)
- **Claude Code skill** — `~/.claude/skills/pixa/SKILL.md` so AI
  coding agents can call pixa automatically

Re-run with `--force` to update after a pixa upgrade.

## Quick start

### Web-optimize an AI-generated image (one command)

```bash
# Source PNG → 1920px WebP (resize + format convert + compress)
pixa compress docs/images/hero.png \
  -o hero.webp --max 1920
# ✓ docs/images/hero.png → hero.webp
#   6.3 MB → 145.0 KB  -97.7%
```

`--max` is the longest edge in pixels. Aspect ratio is preserved, and
the same flag works for both landscape and portrait inputs. You can
reproduce the numbers above against `docs/images/hero.png`,
which is kept in this repo as evidence.

### Compress

```bash
pixa compress photo.jpg                          # → photo.min.jpg
pixa compress photo.jpg -o smaller.jpg           # explicit output
pixa compress ./photos -r                        # → ./photos.min/ (mirrored)
pixa compress logo.png -o logo.webp              # PNG → WebP via extension
pixa compress big.png -o thumb.webp --max 400    # thumbnail
```

When `-o` is omitted, pixa writes to `<input>.min.<ext>` (file) or
`<input>.min/` (directory) — it never overwrites the original. If the
optimizer would make a file *larger* (already-minified assets), the
original bytes are written to the destination instead.

### Split a sheet image into individual files

Input — a single sprite sheet:

<p align="center">
  <img src="docs/images/foxes.webp" alt="Input: 5 fox characters on a single background" width="640">
</p>

One command:

```bash
pixa split foxes.png -o ./avatars \
  --names neutral,happy,thinking,surprised,sleepy
```

Output — 5 individual avatars, text labels automatically excluded, all
padded to the same size so they drop straight into UI components:

<p align="center">
  <img src="docs/images/foxes-output/neutral.webp" alt="neutral" width="110">
  <img src="docs/images/foxes-output/happy.webp" alt="happy" width="110">
  <img src="docs/images/foxes-output/thinking.webp" alt="thinking" width="110">
  <img src="docs/images/foxes-output/surprised.webp" alt="surprised" width="110">
  <img src="docs/images/foxes-output/sleepy.webp" alt="sleepy" width="110">
</p>

Add `--preview` to see exactly what pixa detected — each box shows the
uniform frame the crops were centered on:

<p align="center">
  <img src="docs/images/foxes-preview.webp" alt="Detection preview with colored boxes" width="640">
</p>

How it works:
- Background color is auto-detected from corner samples
- Each object's bounding box is detected; text labels printed below
  characters are excluded automatically
- All output PNGs are uniformly sized — smaller crops are centered on a
  max-sized canvas filled with the detected background color
- When `--names` provides a count, the algorithm re-splits the widest
  blob if the initial detection finds fewer objects than expected
  (handles near-touching or variable-width characters)

Useful flags:
- `--preview` writes `<basename>-preview.png` showing the detection
- `--preview-style detected|output|both` controls what the preview draws
- `--padding 10` adds extra breathing room around each object
- `--transparent` also keys out the detected background color per crop,
  producing RGBA outputs ready to drop onto any UI background

### Make an AI-generated icon transparent

For AI-generated icons, the most reliable way to get a clean transparent
PNG is: ask the model to render the subject on a **solid magenta
(`#FF00FF`)** (or chroma-green) background, then key it out:

```bash
# Chroma-key-friendly prompt (no pink/purple on subject) — defaults
# are tuned for this, no flags needed
pixa transparent fox.png

# Softer / prettier AI prompt (any hue, soft shadows) — the
# "high-quality" combo: narrower flood + edge despill + 1 px erode
pixa transparent fox.png --tolerance 130 --despill --shrink 1

# Override the detected background / pick a specific key colour
pixa transparent fox.png -o fox-alpha.png --bg '#FF00FF'

# Batch a whole directory
pixa transparent ./icons -r -o ./icons-alpha --despill --shrink 1
```

For sheets of multiple icons, combine with `split` — it accepts the
same `--tolerance` / `--despill` / `--shrink` flags:

```bash
pixa split sheet.png -o ./out --names a,b,c --transparent \
    --tolerance 130 --despill --shrink 1
```

| Flag | Default | Purpose |
|---|---|---|
| `--bg <#RRGGBB>` | auto | Override the detected background colour |
| `--tolerance <N>` | 200 | RGB distance threshold for flood-fill |
| `--despill` | off | Channel-based spill suppression on edge band |
| `--despill-band <N>` | 3 | Edge-band radius (pixels) for `--despill` |
| `--shrink <N>` | 0 | Morphological erosion of the opaque region |

The algorithm is a connectivity-based flood fill from the four corners
through pixels within `--tolerance` RGB-distance of the detected
background colour. Flooded pixels become alpha 0; everything else is
left exactly as-is — no colour shifting, no soft alpha halo. A near-bg
pixel buried inside the subject (e.g. a designed pink sparkle) is not
reachable from the corners and survives. Raise `--tolerance` if a halo
remains, lower it and add `--despill --shrink 1` if pastel subject
regions dissolve.

For best results, use a chroma-key-friendly generator prompt that
forbids purple/pink/violet hues on the subject — see
`assets/skills/pixa/SKILL.md` for a copy-paste template.

### Generate a favicon set

```bash
pixa favicon logo.png -o ./public/favicon
```

Outputs:
- `favicon.ico` (multi-resolution: 16×16, 32×32, 48×48)
- `favicon-16x16.png`, `favicon-32x32.png`
- `apple-touch-icon.png` (180×180)
- `android-chrome-192x192.png`, `android-chrome-512x512.png`
- HTML `<link>` snippet to paste into `<head>`

### Convert format only

```bash
pixa convert photo.png photo.webp                # single file
pixa convert ./photos ./out -r --format webp     # directory recursive
```

For most cases, prefer `compress -o foo.webp` since that also re-encodes
the output for size.

### Inspect image

```bash
pixa info photo.jpg                              # human-readable
pixa info photo.jpg --json                       # machine-readable
```

### Remove or detect Gemini watermark

```bash
pixa remove-watermark image.jpg -o clean.jpg
pixa remove-watermark ./photos -r -o ./cleaned --if-detected
pixa detect image.jpg
```

`--if-detected` skips images that don't actually contain a watermark.

## Project layout

```
pixa/
├── Cargo.toml
├── assets/
│   ├── watermark_48x48.png       # embedded chromakey alpha maps
│   ├── watermark_96x96.png
│   └── skills/pixa/SKILL.md      # bundled into the binary by `install`
└── src/
    ├── main.rs                   # CLI entry point
    ├── lib.rs                    # public library API
    ├── compress.rs               # JPEG / PNG / WebP encode + resize
    ├── convert.rs                # format conversion
    ├── favicon.rs                # favicon set generation
    ├── info.rs                   # metadata extraction
    ├── split.rs                  # sheet auto-cropping
    ├── transparent.rs            # chroma-key / color-to-alpha
    ├── watermark.rs              # Reverse Alpha Blending
    └── commands/                 # one file per subcommand
        ├── mod.rs                # shared utilities (walk, format, mirror)
        ├── style.rs              # ANSI color / symbol helpers
        ├── compress.rs
        ├── convert.rs
        ├── detect.rs
        ├── favicon.rs
        ├── info.rs
        ├── install.rs
        ├── remove_watermark.rs
        └── split.rs
```

## How watermark removal works

Gemini composites its visible watermark using:

```
watermarked = α × logo + (1 - α) × original
```

pixa solves for the original pixel values:

```
original = (watermarked - α × logo) / (1 - α)
```

Pre-calibrated 48×48 / 96×96 alpha maps (embedded in the binary) are
used directly, so no AI inference is required. Detection runs in three
stages: Spatial NCC + Gradient NCC + Variance Analysis.

## License

MIT
