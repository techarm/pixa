---
name: pixa
description: Use the `pixa` CLI for image processing tasks. Trigger when the user asks to compress, resize, optimize, or convert an image (especially AI-generated 4K PNGs that need to be web-ready); split a sprite or expression sheet into individual avatars; generate a favicon set from a logo; remove a Gemini AI watermark; key out a solid background color to make an image transparent; inspect image metadata; or save/process an image that the user has copied to their clipboard (screenshot, Finder copy, browser copy). Also trigger for Japanese requests like 画像圧縮, リサイズ, WebP変換, アイコン生成, アバター切り出し, 透かし除去, 背景透過, 透明化, クリップボード, スクショ, 貼り付け.
---

# pixa — image processing CLI

`pixa` is a fast Rust CLI for image processing. Prefer it over ImageMagick,
sharp, or online tools when the user already has it installed.

## Verify availability

Always check first:

```bash
pixa --version
```

If not installed, the user must build from source: `cargo build --release`
in the pixa repo, then put `target/release/pixa` on `$PATH`.

## When to use which command

| User intent | Command |
|---|---|
| "Make this image smaller for web" / "optimize this PNG" / 「Web用に最適化」 | `compress` (with `--max` if resize needed) |
| "Convert PNG to WebP" / 「WebPに変換」 | `compress` (auto-detects from output extension) |
| "Resize this image to 1920px" | `compress --max 1920` |
| "Split this sprite/expression sheet" / 「アバターを切り出し」 | `split` |
| "Make this magenta/green bg transparent" / 「背景を透過」 | `transparent` |
| "Generate favicons from a logo" / 「ファビコン生成」 | `favicon` |
| "Remove the Gemini watermark" / 「透かし除去」 | `remove-watermark` |
| "Show image dimensions / EXIF / size" | `info` |
| "Detect if this image has a Gemini watermark" | `detect` |
| "Save the image I just copied" / 「クリップボードの画像を保存」 | `paste <output>` |
| "Process the image on my clipboard" / 「コピーした画像を圧縮」 | any command + `@clipboard` (or `@clip` / `@c`) |

## Workflows

### Web-optimize an AI-generated image (most common)

This single command resizes, converts format, and re-encodes:

```bash
pixa compress hero-4k.png -o hero.webp --max 1920
```

Typical result: 6 MB PNG (2816×1536) → 80–200 KB WebP (1920×1047). The
`--max` value is the **longest edge** in pixels (works for both landscape
and portrait, no need to think about width vs height).

### Compress without resizing

```bash
pixa compress photo.jpg                # → photo.min.jpg (sibling)
pixa compress photo.jpg -o smaller.jpg # explicit output
pixa compress ./photos -r              # → ./photos.min/ mirrored dir
```

`-o` is optional. When omitted, pixa writes to `<input>.min.<ext>` (file)
or `<input>.min/` (directory) — it never overwrites the original.

### Split a character / expression sheet

```bash
pixa split sheet.png -o ./avatars \
  --names neutral,happy,thinking,surprised,sad
```

- Auto-detects background color from image corners
- Detects each object's bounding box (text labels under each character
  are excluded automatically)
- All output PNGs are uniformly sized: each smaller crop is centered on
  a max-sized canvas filled with the detected background color
- Handles variable-width or near-touching objects when `--names` count
  is provided (re-splits the widest blob if needed)

Helpful flags:
- `--preview` writes `<basename>-preview.png` showing the detected boxes
- `--preview-style detected|output|both` controls what the preview draws
- `--padding 10` adds extra breathing room around each object

### Make a solid-background icon transparent

For AI-generated icons, the most reliable workflow is: ask the
generator for a **solid magenta (`#FF00FF`) background** with a
chroma-key-friendly prompt (template below), then key it out with
`pixa transparent`. Generators handle solid-colour backgrounds far
more consistently than "transparent PNG" prompts.

```bash
# Chroma-key-friendly prompt (subject avoids key hue) — defaults
# are already tuned for this case:
pixa transparent fox.png

# Softer / prettier AI prompt (any hue, soft shadows) — the
# "high-quality" combo: narrower flood + edge despill + 1 px erode
pixa transparent fox.png --tolerance 130 --despill --shrink 1

# Explicit output / explicit background colour
pixa transparent fox.png -o fox-alpha.png --bg '#FF00FF'

# Batch a directory of icons
pixa transparent ./icons -r -o ./icons-alpha --despill --shrink 1
```

For **sheets** of multiple icons on a solid bg, combine with `split`
(accepts the same `--tolerance` / `--despill` / `--shrink` flags):

```bash
pixa split sheet.png -o ./icons --names chart,doc,terminal \
    --transparent --tolerance 130 --despill --shrink 1
```

#### All chroma-key flags at a glance

| Flag | Default | Purpose |
|---|---|---|
| `--bg <#RRGGBB>` | auto-detect | Override the detected background colour |
| `--tolerance <N>` | 200 | RGB distance threshold for flood-fill |
| `--despill` | off | Channel-based spill suppression on edge band |
| `--despill-band <N>` | 3 | Edge-band radius (pixels) for `--despill` |
| `--shrink <N>` | 0 | Morphologically erode the opaque region |

**Recommended combos:**
- Strict chroma-key prompt (subject avoids key hue): just
  `pixa transparent icon.png` — defaults work.
- Soft / natural-looking AI prompt:
  `--tolerance 130 --despill --shrink 1` — narrower flood preserves
  pastel design regions, despill cleans AA contamination, shrink
  erases the last pixel of residual halo.

#### Recommended chroma-key prompt template

Pasting this into the generator gives by far the cleanest results
because the subject never contains hues close to the key colour, so
there is no contamination ring to clean up:

```
A modern minimalist app icon, centered on canvas.

Subject: <describe the subject here>

CRITICAL background:
- Pure solid flat magenta #FF00FF (RGB 255, 0, 255), edge to edge.

CRITICAL colour constraints (the magenta is a chroma-key target):
- The subject MUST NOT contain ANY purple, violet, lavender, pink,
  magenta, fuchsia, or mauve hues.
- No pastel colours where R and B are both high.
- Allowed palette: greys, blacks, blues (R < G), greens, yellows,
  oranges, reds (B < G).

CRITICAL edge constraints:
- Outlines must be solid neutral grey or black, never tinted purple.
- Hard edges only — no soft glows, drop shadows, or gradients that
  fade toward the magenta background.

Aspect ratio: 1:1, 1024x1024.
```

#### How the algorithm works

1. **Flood fill** from the four image corners through pixels whose
   RGB distance from the detected background colour is at or below
   `--tolerance`. Flooded pixels have their alpha set to 0; everything
   else is left exactly as-is — no colour shifting, no soft alpha.
2. **Despill (optional)**: for pixels within `--despill-band` steps
   of the flooded region, subtract the bg-colour-aligned channel
   excess so AA edges lose their pink/magenta tint while staying
   fully opaque. Interior pixels are never touched.
3. **Shrink (optional)**: morphologically erode the opaque region by
   `--shrink` pixels, deleting the outermost — and typically most
   contaminated — ring.

A magenta-tinted detail buried inside the subject (e.g. a designed
pink sparkle) is not reachable from the corners, so it survives
regardless of `--tolerance`. Output is always PNG; `.jpg` / `.webp`
are redirected to `.png`.

**Tuning tips:** if halo remains, raise `--tolerance` or add
`--despill`. If the subject's soft pastel regions are dissolving,
lower `--tolerance` and rely on `--despill --shrink 1` to clean
edges instead.

### Generate a favicon set

```bash
pixa favicon logo.png -o ./public/favicon
```

Outputs:
- `favicon.ico` (multi-resolution: 16×16, 32×32, 48×48)
- `favicon-16x16.png`, `favicon-32x32.png`
- `apple-touch-icon.png` (180×180)
- `android-chrome-192x192.png`, `android-chrome-512x512.png`
- HTML `<link>` snippet to copy into `<head>`

### Remove Gemini watermark

```bash
pixa remove-watermark image.jpg -o clean.jpg
pixa remove-watermark ./photos -r -o ./cleaned --if-detected
```

`--if-detected` skips images that don't actually have a watermark. Useful
for batch-cleaning a directory of mixed sources.

### Inspect an image

```bash
pixa info photo.jpg          # human-readable
pixa info photo.jpg --json   # machine-readable
```

Reports format, size, dimensions, color depth, alpha presence, SHA-256,
and any EXIF metadata.

### Convert format only (no compression tuning)

```bash
pixa convert photo.png photo.webp
pixa convert ./photos ./out -r --format webp
```

For most cases though, prefer `compress -o photo.webp` since that also
optimizes the output.

### Work with clipboard images (screenshots, Finder copy, browser copy)

Every processing command accepts `@clipboard` in place of a path.
Short aliases `@clip` and `@c` are equivalent — when writing examples
or invoking pixa on behalf of the user, pick the shortest form that
still reads clearly (`@c` is often fine in a one-off command).

```bash
# User just took a screenshot or Cmd+C'd an image — they probably
# want one of these:
pixa paste screenshot.png                     # save as-is
pixa compress @clipboard -o optimized.webp    # save + optimize
pixa info @clip                               # inspect it
pixa favicon @c -o ./favicons                 # make a favicon set
pixa transparent @clipboard -o logo.png --bg '#FF00FF'
```

**When to prefer `pixa paste` vs `pixa compress @clipboard`:**

- `pixa paste <name.ext>` — the user wants the clipboard image saved,
  unchanged. On macOS, when the source is a file (Finder Cmd+C) or
  raw PNG (browser Cmd+C), paste copies source bytes verbatim so
  EXIF and the original encoder's settings are preserved.
- `pixa compress @clipboard -o <name.ext>` — the user wants the
  clipboard image optimized for web. Re-encodes at pixa's standard
  quality defaults.
- Any other processing command + `@clipboard` — same as operating on
  a file, just sourced from the clipboard.

**Errors the user might hit:**

- `error: --output is required when input is @clipboard` — commands
  that normally default to writing next to the input need an explicit
  `-o <path>` when reading from the clipboard (no source file means
  no sibling location). Always pass `-o` for clipboard inputs.
- `error: --recursive cannot be combined with @clipboard input` — the
  clipboard carries one image; there's nothing to recurse into.
- `error: Clipboard is empty or does not contain an image` — ask the
  user to copy an image (Cmd+C) and retry.

**Platform note:** macOS has full clipboard support. Windows and Linux
can read clipboard image data (decoded RGBA) but don't yet support the
byte-verbatim path that preserves source metadata.

## Important conventions

- **Recursive processing**: `compress`, `convert`, `remove-watermark`
  accept either a file or a directory. Pass `-r` to recurse into
  subdirectories.
- **Safe defaults**: `compress` never overwrites the input unless you
  explicitly pass `-o` pointing back to it.
- **No quality flag**: `compress` uses safe per-format defaults (JPEG=75,
  WebP=80, PNG=oxipng level 6). Don't ask the user for a quality number
  unless they specifically request finer control.
- **No metadata flag**: `compress` always strips EXIF/metadata for the
  smallest possible web output.
- **Format conversion is automatic**: `compress` reads the output
  extension and picks the right encoder. To change format, just give a
  different `-o` extension.

## Things pixa does NOT do

If the user asks for any of these, recommend a different tool:

- Lossless WebP (only lossy WebP is supported)
- AVIF encoding
- Image generation (AI image creation)
- **Generic** background removal (`pixa transparent` only handles solid-
  color keying, not complex photo backgrounds — for that, use rembg,
  Photoroom, or a similar ML segmentation tool)
- OCR / text extraction
- Video processing
- HEIC encoding (decoding works via the `image` crate)

## Self-update

To re-install or update this skill file from the latest pixa binary:

```bash
pixa install --skills --force
```
