#!/usr/bin/env bash
# post-merge on main: publish the frontend to GitHub Pages if the pull changed it.
set -euo pipefail

[[ "$(git rev-parse --abbrev-ref HEAD)" == "main" ]] || exit 0

if git diff --name-only ORIG_HEAD HEAD -- frontend/ | grep -q .; then
  echo "frontend changed on main -> publishing to GitHub Pages"
  ./scripts/deploy-pages.sh
else
  echo "no frontend changes, Pages untouched"
fi
