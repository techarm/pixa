.DEFAULT_GOAL := help

.PHONY: help fmt fmt-check lint test check build run clean install release

help:          ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

fmt:           ## Format all Rust code
	cargo fmt --all

fmt-check:     ## Verify formatting without modifying files
	cargo fmt --all -- --check

lint:          ## Run clippy with warnings treated as errors
	cargo clippy --all-targets -- -Dwarnings

test:          ## Run the full test suite
	cargo test --all-features

check: fmt-check lint test  ## Run every CI check locally (fmt + lint + test)

build:         ## Build the release binary at target/release/pixa
	cargo build --release

run:           ## Run pixa with arguments: make run ARGS="compress photo.jpg"
	cargo run --release -- $(ARGS)

install:       ## Install the pixa binary into ~/.cargo/bin
	cargo install --path . --force

clean:         ## Remove build artifacts
	cargo clean

release:       ## Bump version, tag, and push: make release VERSION=0.1.3
	@test -n "$(VERSION)" || (echo "Usage: make release VERSION=x.y.z" && exit 1)
	sed -i.bak 's/^version = .*/version = "$(VERSION)"/' Cargo.toml && rm Cargo.toml.bak
	cargo build --release
	git add Cargo.toml Cargo.lock
	git commit -m "chore: release v$(VERSION)"
	git tag -a v$(VERSION) -m "v$(VERSION)"
	git push && git push origin v$(VERSION)
	@echo ""
	@echo "Tag pushed. CI will build prebuilt binaries and update Homebrew tap."
	@echo "Remember to also run: cargo publish"
