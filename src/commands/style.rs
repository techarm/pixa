//! Terminal styling helpers with NO_COLOR + TTY detection.
//!
//! Helpers come in two flavours so each output stream's TTY status is
//! checked independently:
//!
//! - The top-level helpers (`green`, `fail_mark`, `arrow`, etc.) check
//!   stdout's TTY state. Use them for `println!` callsites.
//! - The [`err`] submodule mirrors the same helpers but checks
//!   stderr's TTY state. Use them for `eprintln!` callsites — this
//!   keeps errors coloured when the user pipes stdout (`pixa … > out`)
//!   but leaves stderr attached to a TTY.
//!
//! All helpers return plain text when the relevant stream is not a
//! TTY, when `NO_COLOR` is set, or when `TERM=dumb`. Color can be
//! force-enabled (for screenshots, CI logs, etc.) by setting
//! `FORCE_COLOR=1` or `CLICOLOR_FORCE=1` — those overrides apply to
//! both streams.
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

#[derive(Copy, Clone, PartialEq)]
enum Stream {
    Stdout,
    Stderr,
}

fn color_enabled_for(stream: Stream) -> bool {
    static STDOUT: OnceLock<bool> = OnceLock::new();
    static STDERR: OnceLock<bool> = OnceLock::new();
    let cell = match stream {
        Stream::Stdout => &STDOUT,
        Stream::Stderr => &STDERR,
    };
    *cell.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        if matches!(std::env::var("TERM").as_deref(), Ok("dumb")) {
            return false;
        }
        if std::env::var_os("FORCE_COLOR").is_some() || std::env::var_os("CLICOLOR_FORCE").is_some()
        {
            return true;
        }
        match stream {
            Stream::Stdout => std::io::stdout().is_terminal(),
            Stream::Stderr => std::io::stderr().is_terminal(),
        }
    })
}

fn paint_rgb_when(rgb: (u8, u8, u8), s: &str, enabled: bool) -> String {
    if enabled {
        format!("\x1b[38;2;{};{};{}m{s}\x1b[0m", rgb.0, rgb.1, rgb.2)
    } else {
        s.to_string()
    }
}

fn paint_sgr_when(code: &str, s: &str, enabled: bool) -> String {
    if enabled {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

// --- stdout-bound helpers (use with println!) ---

pub fn green(s: &str) -> String {
    paint_rgb_when(SAGE, s, color_enabled_for(Stream::Stdout))
}
pub fn red(s: &str) -> String {
    paint_rgb_when(CORAL, s, color_enabled_for(Stream::Stdout))
}
pub fn yellow(s: &str) -> String {
    paint_rgb_when(AMBER, s, color_enabled_for(Stream::Stdout))
}
pub fn cyan(s: &str) -> String {
    paint_rgb_when(TEAL, s, color_enabled_for(Stream::Stdout))
}
pub fn dim(s: &str) -> String {
    paint_sgr_when("2", s, color_enabled_for(Stream::Stdout))
}
pub fn bold(s: &str) -> String {
    paint_sgr_when("1", s, color_enabled_for(Stream::Stdout))
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

// --- stderr-bound helpers (use with eprintln!) ---

/// Stderr-bound style helpers. Use these inside `eprintln!` so colour
/// stays visible when stdout is piped (`pixa … > out.json`) but
/// stderr is still attached to a TTY. The functions are otherwise
/// identical to the top-level ones — just keyed off
/// `std::io::stderr().is_terminal()` instead of stdout.
pub mod err {
    use super::{AMBER, CORAL, SAGE, Stream, color_enabled_for, paint_rgb_when, paint_sgr_when};

    pub fn green(s: &str) -> String {
        paint_rgb_when(SAGE, s, color_enabled_for(Stream::Stderr))
    }
    pub fn red(s: &str) -> String {
        paint_rgb_when(CORAL, s, color_enabled_for(Stream::Stderr))
    }
    pub fn yellow(s: &str) -> String {
        paint_rgb_when(AMBER, s, color_enabled_for(Stream::Stderr))
    }
    pub fn dim(s: &str) -> String {
        paint_sgr_when("2", s, color_enabled_for(Stream::Stderr))
    }
    pub fn fail_mark() -> String {
        red("✗")
    }
}

/// Git-style fatal error prefix, e.g. `error:` in bold coral.
/// Used for one-line errors bubbled up to `main()` (which writes to
/// stderr), so the colour decision is keyed off stderr's TTY status.
///
/// Bold + truecolor are emitted as a *single* combined SGR sequence
/// (`\x1b[1;38;2;R;G;Bm…\x1b[0m`). Wrapping `red("error:")` in
/// `paint_sgr("1", …)` would put the bold reset *after* `red`'s
/// inner `\x1b[0m`, which cancels the bold attribute prematurely
/// and leaves `error:` rendered without weight.
pub fn error_prefix() -> String {
    if color_enabled_for(Stream::Stderr) {
        let (r, g, b) = CORAL;
        format!("\x1b[1;38;2;{r};{g};{b}merror:\x1b[0m")
    } else {
        "error:".to_string()
    }
}

/// Git-style hint prefix, e.g. `hint:` in dim. Goes to stderr below
/// `error:`, so it shares the stderr-keyed colour decision.
pub fn hint_prefix() -> String {
    err::dim("hint:")
}
