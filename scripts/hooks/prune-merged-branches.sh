#!/usr/bin/env bash
# post-merge on main: delete local branches that were merged and deleted on GitHub.
set -euo pipefail

[[ "$(git rev-parse --abbrev-ref HEAD)" == "main" ]] || exit 0

# A gone upstream alone is not proof of a merge (a PR closed unmerged, a commit
# made after the merge). Rebase-merge rewrites hashes, so compare patches:
# `git cherry` marks with `+` every commit whose change is not in main.
git fetch --prune --quiet origin
git for-each-ref --format='%(refname:short) %(upstream:track)' refs/heads \
  | awk '$2 == "[gone]" && $1 != "main" {print $1}' \
  | while read -r branch; do
      if git cherry origin/main "$branch" | grep -q '^+'; then
        echo "$branch: deleted on GitHub but has commits not in main -> kept"
      else
        echo "$branch was merged and deleted on GitHub -> deleting it locally"
        git branch -D "$branch"
      fi
    done
