# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.6](https://github.com/techarm/pixa/compare/v0.1.5...v0.1.6) - 2026-04-15

### Other

- *(CONVERT)* PRESERVE ALPHA CHANNEL WHEN CONVERTING TO WEBP (#12)

## [0.1.5](https://github.com/techarm/pixa/compare/v0.1.4...v0.1.5) - 2026-04-15

### Added

- **`transparent`** — key out a solid background colour (chroma-key) from single images or directories, using a connectivity-based flood fill from the four corners. Optional `--despill` for channel-based edge spill suppression and `--shrink N` for morphological erosion of the outermost ring. ([#9](https://github.com/techarm/pixa/pull/9))
- **`split --transparent`** — same chroma-key treatment applied per cropped object from a sheet. Accepts the same `--tolerance` / `--despill` / `--shrink` tuning flags. ([#9](https://github.com/techarm/pixa/pull/9))
- **Chroma-key prompt template** in the bundled Claude Code skill so agents can ask AI generators for images that key cleanly out of the box. ([#9](https://github.com/techarm/pixa/pull/9))

## [0.1.4] - 2026-04-09

### Features

- **Shell completions** — `pixa completions <shell>` generates completion scripts for bash/zsh/fish/elvish/PowerShell. `pixa install --completions` auto-detects your shell and installs to the standard directory. ([#7](https://github.com/techarm/pixa/pull/7))
- **CLAUDE.md** — project rules for AI coding agents: conventional commits, branching, release process, code style. ([#5](https://github.com/techarm/pixa/pull/5))

### Improvements

- Widen README hero banner from 640px to 800px for better readability.
- Fix changelog template to avoid redundant heading levels.

## [0.1.3] - 2026-04-09

First changelog-tracked release. Includes all features built since v0.1.0.

### Features

- **compress** — one-command web optimization: resize + format convert + compress with `--max` flag. Auto-naming (`photo.min.jpg`) when `-o` is omitted. Keeps original if compressed size is larger.
- **split** — auto-detect and crop individual objects from a sprite/expression sheet. Background color detection, text label exclusion, uniform output sizing with background-color padding, preview with `--preview-style detected|output|both`.
- **favicon** — generate a complete web-ready icon set (ICO multi-res + 5 PNG sizes + HTML snippet) from any image.
- **remove-watermark** — remove Gemini AI watermarks via Reverse Alpha Blending. `--if-detected` flag to skip clean images.
- **detect** — score whether a Gemini watermark is present.
- **convert** — convert between JPEG, PNG, WebP, BMP, GIF, TIFF.
- **info** — show dimensions, color, EXIF, SHA-256 metadata. `--json` for machine-readable output.
- **install --skills** — install a Claude Code skill so coding agents can use pixa automatically.
- **Brand palette** — all CLI output uses a fixed 24-bit truecolor palette (sage/coral/teal/amber) for consistent appearance across terminal themes. Respects `NO_COLOR` and `FORCE_COLOR`.

### Testing & CI

- 62 tests total: 41 unit tests + 21 CLI integration tests via `assert_cmd`.
- GitHub Actions CI on every push/PR: `make check` (fmt + clippy + test).
- Makefile with `check`, `build`, `release`, and other common targets.

### Distribution

- **Homebrew**: `brew tap techarm/tap && brew install pixa`
- **Prebuilt binaries**: macOS (Intel + Apple Silicon), Linux (x86_64 + ARM64), Windows (x86_64) via cargo-dist.
- **Shell / PowerShell installers**: one-line install scripts in GitHub Releases.
- **crates.io**: `cargo install pixa`
- **Claude Code skill**: `pixa install --skills`

### Release automation

- **release-plz**: auto-generates release PRs with version bump + CHANGELOG on every push to main.
- **cargo-dist**: builds cross-platform binaries + updates Homebrew tap on every tag push.
- Fully automated pipeline: merge release PR → tag → binaries → Homebrew → crates.io.
