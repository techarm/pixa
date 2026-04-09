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

| Prefix | Version bump | Example |
|---|---|---|
| `feat:` | minor (0.1.x → 0.2.0) | `feat: add resize command` |
| `fix:` | patch (0.1.3 → 0.1.4) | `fix: compress panics on empty file` |
| `docs:` | none | `docs: update README` |
| `test:` | none | `test: add split edge case` |
| `ci:` | none | `ci: add macOS to matrix` |
| `chore:` | none | `chore: update dependencies` |
| `refactor:` | none | `refactor: extract helper` |

Scope is optional: `feat(split):`, `fix(compress):`, etc.

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
make test     # run all tests (62+ currently)
```

- Unit tests live in each `src/*.rs` via `#[cfg(test)]`.
- Integration tests live in `tests/cli_*.rs` via `assert_cmd`.
- Test images are generated in-memory (no binary fixtures committed).
