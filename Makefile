# svp — local CI. The prek hooks (prek.toml) call these targets; run them by hand any time.
# There is intentionally no hosted CI: see the wiki, https://github.com/sbOogway/svp/wiki/Architecture#workflow

.PHONY: setup fmt lint test audit build ci ci-fast ci-clean

setup:
	@command -v prek >/dev/null || { echo "prek is required: https://prek.j178.dev/installation/" >&2; exit 1; }
	git config --unset core.hooksPath || true
	prek install --overwrite

fmt: 
	cargo fmt --all --check

lint: 
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

audit:
	@if command -v cargo-audit >/dev/null; then cargo audit; else echo "cargo-audit not installed: cargo install cargo-audit --locked"; fi

build:
	cargo build --workspace --release

ci-fast: fmt lint

ci: ci-fast test audit

ci-clean: 
	cargo clean
	$(MAKE) setup ci
