//! User-facing error type with optional hint lines.
//!
//! `main()` intercepts every command's `Result<()>` and renders errors
//! as one-line `error: <msg>` in git style, optionally followed by
//! `hint: <...>` lines. Commands that want to suggest a next action
//! attach hints via [`bail_with_hints`] or construct a [`CliError`]
//! directly.
//!
//! The rest of the codebase continues to use `anyhow::Error` freely —
//! `main()` unwraps a [`CliError`] if present, otherwise falls back to
//! the anyhow error's top-level `Display` (no `Caused by:` chain).

use std::fmt;

/// An error with a short user-facing message and zero or more hint
/// lines. Wrapped in `anyhow::Error` when propagated via `?`, then
/// unpacked by `main()` for display.
#[derive(Debug, Clone)]
pub struct CliError {
    pub message: String,
    pub hints: Vec<String>,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CliError {}

/// Build an `anyhow::Error` carrying a message plus one or more hint
/// lines. Equivalent to `anyhow::anyhow!(...)` but with structured
/// follow-up text that `main()` renders separately.
pub fn bail_with_hints<I, S>(msg: impl Into<String>, hints: I) -> anyhow::Error
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    CliError {
        message: msg.into(),
        hints: hints.into_iter().map(Into::into).collect(),
    }
    .into()
}

/// Extract the user-facing message and hint list from an anyhow error.
/// If the error is (or wraps) a `CliError`, its structured fields are
/// returned. Otherwise the anyhow chain is flattened into a single
/// line, skipping any layer whose `Display` is already a suffix of the
/// previous layer — this avoids `"Foo: bar: bar"` artifacts when an
/// error type's `Display` includes its inner's message verbatim.
pub fn unpack(err: &anyhow::Error) -> (String, Vec<String>) {
    if let Some(cli) = err.downcast_ref::<CliError>() {
        return (cli.message.clone(), cli.hints.clone());
    }
    let mut parts: Vec<String> = Vec::new();
    for cause in err.chain() {
        let s = cause.to_string();
        if s.is_empty() {
            continue;
        }
        // Skip a layer if the previous one already ends with its
        // message (e.g. ImageError's Display embeds the inner's).
        if let Some(prev) = parts.last()
            && (prev == &s || prev.ends_with(&format!(": {s}")))
        {
            continue;
        }
        parts.push(s);
    }
    (parts.join(": "), Vec::new())
}
