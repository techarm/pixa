# pixa — Project Rules

## Build & Check

```bash
make check    # fmt + clippy + test — must pass before any PR
make build    # release build
make help     # list all targets
```

## Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/). The
prefix determines whether a release is triggered:

| Prefix | Version bump (0.x) | Version bump (≥1.0) | Example |
|---|---|---|---|
| `feat!:` | minor (0.1.x → 0.2.0) | major | `feat!: drop --legacy flag` |
| `feat:` | patch (0.1.3 → 0.1.4) | minor | `feat: add resize command` |
| `fix:` | patch | patch | `fix: compress panics on empty file` |
| `docs:` | none | none | `docs: update README` |
| `test:` | none | none | `test: add split edge case` |
| `ci:` | none | none | `ci: add macOS to matrix` |
| `chore:` | none | none | `chore: update dependencies` |
| `refactor:` | none | none | `refactor: extract helper` |

Scope is optional: `feat(split):`, `fix(compress):`, etc.

A commit is also considered breaking — and gets the same bump as a
`!` prefix — if its **body** contains a `BREAKING CHANGE: <what>`
footer. Either form works, pick whichever reads better:

```
# Prefix form
feat!: drop --legacy flag

# Footer form (preferred when the explanation is long)
feat: drop --legacy flag

BREAKING CHANGE: --legacy is no longer accepted. Use --modern instead.
```

> **0.x pre-release note.** release-plz follows semver's "anything
> can change in 0.x" rule: `feat:` and `fix:` both trigger a patch
> bump. Only breaking-change commits (`feat!:` or a `BREAKING CHANGE:`
> footer) bump the minor number while we are pre-1.0. Once the crate
> goes 1.0+, `feat:` starts triggering minor bumps as usual.

## Branching

- **Never push directly to main** — always create a feature branch and PR.
- Branch names: `feat/...`, `fix/...`, `docs/...`, `test/...`
- PRs are **squash-merged** (only squash is enabled on this repo).

## Release Process

Releases are **fully automated** via release-plz + cargo-dist:

1. `feat:` or `fix:` commits land on main via PR.
2. release-plz auto-creates a release PR with version bump + CHANGELOG.
3. Merge the release PR → tag, crates.io publish, GitHub Release, and
   Homebrew tap update all happen automatically.

**Do NOT manually edit `version` in Cargo.toml or create git tags.**

## Code Style

- `cargo fmt` — enforced in CI.
- `cargo clippy -Dwarnings` — no warnings allowed.
- Truecolor brand palette in `src/commands/style.rs`:
  sage (#7FB069), coral (#E67E5E), amber (#D9A55C), teal (#6BA4A0).
- Prefer `green()` for input paths, `red()` for sizes/results,
  `cyan()` for info values, `dim()` for secondary text.

## Project Structure

- `src/*.rs` — library modules (compress, convert, split, etc.)
- `src/commands/*.rs` — CLI wrappers (one per subcommand)
- `tests/cli_*.rs` — integration tests (one per subcommand)
- `tests/common/mod.rs` — shared test fixtures
- `assets/` — embedded files (watermark masks, skills)
- `docs/images/` — README images

## Testing

```bash
make test     # run all tests
```

- Unit tests live in each `src/*.rs` via `#[cfg(test)]`.
- Integration tests live in `tests/cli_*.rs` via `assert_cmd`.
- Test images are generated in-memory (no binary fixtures committed).

## Bug Fix Checklist — MANDATORY

Before opening any `fix:` PR, every step below MUST be performed. Do
not skip. This checklist exists because a real bug (#12) shipped
with only one of two affected code paths fixed, forcing a second
release cycle and wasting the user's time.

**1. Grep every call site of the affected primitive.**
When fixing a bug in how we call a library API (codec, encoder,
decoder, parser, IO helper, etc.), the first action is a repo-wide
search for every call to that same API. Audit each hit. If the
bug applies there too, fix it in the same PR.

```bash
# Example: the #12 bug was `webp::Encoder::from_rgb` dropping alpha.
# Before shipping, this grep should have turned up BOTH sites:
rg 'webp::Encoder::from_rgb\b' src/
# → src/convert.rs  AND  src/compress.rs
```

The pattern generalises: whenever a fix changes how a primitive
(`image::open`, `to_rgb8`, `DynamicImage::save`, `webp::Encoder::*`,
`mozjpeg::Compress::*`, `oxipng::optimize*`, filesystem calls, etc.)
is invoked, grep for other call sites of the same primitive and
audit every one before opening the PR.

**2. Write a symptom-level integration test, not just a unit test.**
A unit test on one internal function only proves that one function
is correct — it does not prove the user-visible invariant holds
across every command that can exhibit the symptom. Add an
integration test in `tests/cli_*.rs` that invokes the pixa CLI
exactly as a user would and asserts the symptom does not recur.

For #12, the right test would have been:
*"`pixa compress transparent.png -o out.webp` produces a WebP
whose alpha channel is preserved"* — which exercises `compress.rs`
and would have failed on 0.1.6, catching the second bug before
release. A unit test bound to `convert_image` alone (as #12 shipped)
could not have caught it.

**3. Re-read the original report, not just the first code path you
found.** The user's words describe a *symptom*; the first code path
you match is a *hypothesis*. Before declaring a fix complete, ask:
"does any other command or code path produce this same symptom?"
If yes, they are all in scope for this PR.
