# svp — local CI. The prek hooks (prek.toml) call these targets; run them by hand any time.
# There is intentionally no hosted CI: see the wiki, https://github.com/sbOogway/svp/wiki/Architecture#workflow

.PHONY: setup fmt lint test audit build ci ci-fast ci-clean

setup:
	@command -v prek >/dev/null || { echo "prek is required: https://prek.j178.dev/installation/" >&2; exit 1; }
	git config --unset core.hooksPath || true
	prek install --overwrite

fmt: 
	cd backend && cargo fmt --all --check

lint: 
	cd backend && cargo clippy --workspace --all-targets -- -D warnings

test:
	cd backend && cargo test --workspace

audit:
	@if command -v cargo-audit >/dev/null; then cd backend && cargo audit; else echo "cargo-audit not installed: cargo install cargo-audit --locked"; fi

build:
	cd backend && cargo build --workspace --release

ci-fast: fmt lint

ci: ci-fast test audit

ci-clean: 
	cd backend && cargo clean
	$(MAKE) setup ci
