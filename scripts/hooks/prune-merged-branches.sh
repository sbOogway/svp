#!/usr/bin/env bash
# post-merge on main: delete local branches that were merged and deleted on GitHub.
set -euo pipefail

[[ "$(git rev-parse --abbrev-ref HEAD)" == "main" ]] || exit 0

# Rebase-merge rewrites the commits, so `git branch -d` would refuse:
# a gone upstream is the merge signal.
git fetch --prune --quiet origin
git for-each-ref --format='%(refname:short) %(upstream:track)' refs/heads \
  | awk '$2 == "[gone]" && $1 != "main" {print $1}' \
  | while read -r branch; do
      echo "$branch was merged and deleted on GitHub -> deleting it locally"
      git branch -D "$branch"
    done
