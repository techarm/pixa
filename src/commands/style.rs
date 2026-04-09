//! Terminal styling helpers with NO_COLOR + TTY detection.
//!
//! All helpers return plain text when stdout is not a TTY, when
//! the `NO_COLOR` env var is set, or when `TERM=dumb`. Color can be
//! force-enabled (for screenshots, CI logs, etc.) by setting
//! `FORCE_COLOR=1` or `CLICOLOR_FORCE=1`.
//!
//! Colors are rendered as 24-bit truecolor ANSI sequences using a
//! fixed brand palette (warm sage / coral / amber / teal), so the
//! output looks the same regardless of the user's terminal theme.
//! Almost all modern terminals (iTerm2, kitty, alacritty, Windows
//! Terminal, VS Code integrated, WezTerm, etc.) support truecolor.

use std::io::IsTerminal;
use std::sync::OnceLock;

// Brand palette — keep these in sync with assets/skills/pixa/SKILL.md
// and the hero image in docs/images/.
const SAGE: (u8, u8, u8) = (127, 176, 105); // #7FB069 — success
const CORAL: (u8, u8, u8) = (230, 126, 94); // #E67E5E — error / accent
const AMBER: (u8, u8, u8) = (217, 165, 92); // #D9A55C — warning
const TEAL: (u8, u8, u8) = (107, 164, 160); // #6BA4A0 — info

fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        if matches!(std::env::var("TERM").as_deref(), Ok("dumb")) {
            return false;
        }
        if std::env::var_os("FORCE_COLOR").is_some()
            || std::env::var_os("CLICOLOR_FORCE").is_some()
        {
            return true;
        }
        std::io::stdout().is_terminal()
    })
}

fn paint_rgb(rgb: (u8, u8, u8), s: &str) -> String {
    if color_enabled() {
        format!("\x1b[38;2;{};{};{}m{s}\x1b[0m", rgb.0, rgb.1, rgb.2)
    } else {
        s.to_string()
    }
}

fn paint_sgr(code: &str, s: &str) -> String {
    if color_enabled() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn green(s: &str) -> String {
    paint_rgb(SAGE, s)
}
pub fn red(s: &str) -> String {
    paint_rgb(CORAL, s)
}
pub fn yellow(s: &str) -> String {
    paint_rgb(AMBER, s)
}
pub fn cyan(s: &str) -> String {
    paint_rgb(TEAL, s)
}
pub fn dim(s: &str) -> String {
    paint_sgr("2", s)
}
pub fn bold(s: &str) -> String {
    paint_sgr("1", s)
}

/// Success mark, e.g. "✓" (or plain "OK" when colors disabled).
pub fn ok_mark() -> String {
    green("✓")
}
pub fn fail_mark() -> String {
    red("✗")
}
pub fn skip_mark() -> String {
    yellow("-")
}
pub fn arrow() -> String {
    dim("→")
}
