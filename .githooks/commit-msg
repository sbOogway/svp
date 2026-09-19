#!/usr/bin/env bash
# Enforce Conventional Commits: type(scope)!: subject
set -euo pipefail
msg=$(head -n1 "$1")
pattern='^(feat|fix|docs|style|refactor|perf|test|build|chore|ci|revert)(\([a-z0-9._/-]+\))?!?: .{1,72}$'
if [[ "$msg" =~ ^(Merge|Revert|fixup!|squash!) ]]; then exit 0; fi
if ! [[ "$msg" =~ $pattern ]]; then
  echo "commit-msg: subject must follow Conventional Commits, e.g. 'feat(core): add bar accumulator'" >&2
  echo "  got: $msg" >&2
  exit 1
fi
