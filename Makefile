# svp — local CI. The prek hooks (prek.toml) call these targets; run them by hand any time.
# There is intentionally no hosted CI: see the wiki, https://github.com/sbOogway/svp/wiki/Architecture#workflow

.PHONY: help setup fmt lint typecheck test audit build ci ci-fast ci-clean deploy-pages

help:
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

setup:
	@command -v prek >/dev/null || { echo "prek is required: https://prek.j178.dev/installation/" >&2; exit 1; }
	git config --unset core.hooksPath || true
	prek install --overwrite
	cd frontend && npm ci --no-fund --no-audit

fmt: 
	cd backend && cargo fmt --all --check

lint: 
	cd backend && cargo clippy --workspace --all-targets -- -D warnings
	cd frontend && npm run lint

typecheck:
	cd frontend && npm run typecheck

test:
	cd backend && cargo test --workspace

audit:
	@if command -v cargo-audit >/dev/null; then cd backend && cargo audit; else echo "cargo-audit not installed: cargo install cargo-audit --locked"; fi
	cd frontend && npm audit --audit-level=high

build:
	cd backend && cargo build --workspace --release
	cd frontend && npm run build

ci-fast: fmt lint typecheck 

ci: ci-fast test audit build 

ci-clean: 
	cd backend && cargo clean
	rm -rf frontend/node_modules frontend/dist
	$(MAKE) setup ci

deploy-pages: 
	./scripts/deploy-pages.sh
