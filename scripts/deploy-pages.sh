#!/usr/bin/env bash
# Publish frontend/dist to the gh-pages branch (GitHub Pages "deploy from branch").
# Called by the pre-push hook when pushing main, or by hand via `make deploy-pages`.
set -euo pipefail
cd "$(dirname "$0")/../frontend"

: "${VITE_BASE_PATH:=/svp/}"
export VITE_BASE_PATH
# Production values come from frontend/.env.production (committed, no secrets).
npm run build
npm run deploy:pages
echo "published to gh-pages (base path ${VITE_BASE_PATH})"
