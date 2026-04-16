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
/// If any layer in the error chain is a `CliError`, its structured
/// fields are returned — walking the chain (rather than only checking
/// the top) lets hints survive `.context()` wrapping. Otherwise the
/// chain is flattened into a single line, skipping any layer whose
/// `Display` is already a suffix of the previous layer to avoid
/// `"Foo: bar: bar"` artifacts when an error type's `Display`
/// includes its inner's message verbatim.
pub fn unpack(err: &anyhow::Error) -> (String, Vec<String>) {
    for cause in err.chain() {
        if let Some(cli) = cause.downcast_ref::<CliError>() {
            return (cli.message.clone(), cli.hints.clone());
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpack_extracts_cli_error_fields() {
        let err = bail_with_hints("boom", ["try X", "or Y"]);
        let (msg, hints) = unpack(&err);
        assert_eq!(msg, "boom");
        assert_eq!(hints, vec!["try X".to_string(), "or Y".to_string()]);
    }

    #[test]
    fn unpack_walks_chain_to_find_cli_error_under_context() {
        // A CliError with hints, wrapped by anyhow::Context. Without
        // chain-walking the hints would be silently dropped.
        let err: anyhow::Error =
            bail_with_hints("underlying", ["one", "two"]).context("while doing X");
        let (msg, hints) = unpack(&err);
        assert_eq!(msg, "underlying");
        assert_eq!(hints, vec!["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn unpack_falls_back_to_flattened_chain_for_non_cli_errors() {
        let inner = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err: anyhow::Error = anyhow::Error::new(inner).context("could not read");
        let (msg, hints) = unpack(&err);
        assert_eq!(msg, "could not read: missing");
        assert!(hints.is_empty());
    }
}
