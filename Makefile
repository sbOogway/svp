# svp — local CI. The git hooks call these targets; run them by hand any time.
# There is intentionally no hosted CI: see docs/ARCHITECTURE.md § "Workflow".

.PHONY: help setup fmt lint typecheck test audit build ci ci-fast ci-clean deploy-pages

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

setup: ## One-time per clone: install git hooks and frontend deps
	git config core.hooksPath .githooks
	cd frontend && npm ci --no-fund --no-audit
	@echo "hooks installed (core.hooksPath=.githooks)"

fmt: ## Rust formatting check
	cd backend && cargo fmt --all --check

lint: ## Rust clippy (deny warnings) + frontend oxlint
	cd backend && cargo clippy --workspace --all-targets -- -D warnings
	cd frontend && npm run lint

typecheck: ## Frontend TypeScript check
	cd frontend && npm run typecheck

test: ## Rust tests
	cd backend && cargo test --workspace

audit: ## Dependency vulnerability audit (skips tools that are not installed)
	@if command -v cargo-audit >/dev/null; then cd backend && cargo audit; else echo "cargo-audit not installed: cargo install cargo-audit --locked"; fi
	cd frontend && npm audit --audit-level=high

build: ## Release build of backend and frontend
	cd backend && cargo build --workspace --release
	cd frontend && npm run build

ci-fast: fmt lint typecheck ## What pre-commit runs (seconds)

ci: ci-fast test audit build ## What pre-push runs (minutes on first build)

ci-clean: ## Full CI from a clean state (run before tagging a release)
	cd backend && cargo clean
	rm -rf frontend/node_modules frontend/dist
	$(MAKE) setup ci

deploy-pages: ## Build the frontend for GitHub Pages and push it to the gh-pages branch
	./scripts/deploy-pages.sh
