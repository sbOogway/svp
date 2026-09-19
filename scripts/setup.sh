#!/usr/bin/env bash
# One-time developer setup: toolchain sanity, git hooks, frontend deps.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v rustup >/dev/null; then
  echo "rustup is required (https://rustup.rs). The toolchain is pinned in rust-toolchain.toml." >&2
  exit 1
fi
rustup show active-toolchain
command -v node >/dev/null || { echo "node >= 22 is required" >&2; exit 1; }

make setup
echo "done. Try: make ci-fast"
