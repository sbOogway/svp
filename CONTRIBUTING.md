# Contributing

1. Install [prek](https://prek.j178.dev/installation/), then run `./scripts/setup.sh`
   once per clone (installs the hooks; nothing works without them).
2. Pick or open an issue; every PR references one and targets a milestone.
3. Branch from `main`: `<type>/<short-name>` (e.g. `feat/bar-accumulator`).
4. Commits follow [Conventional Commits](https://www.conventionalcommits.org):
   `feat(aggregator): …`, `fix(server): …`, `docs: …`. Scopes: `common`, `aggregator`,
   `server`, `client`, `app`, `infra`, `docs`.
5. `make ci` must pass locally — the `pre-push` hook enforces it. There is no hosted CI.
   CI never builds: it checks formatting, lints, tests and audits only.
   Do not add `build` (or any release build) to `ci` or to the hooks; run
   `make build` by hand when you need the artifacts.
6. Don't overcomment. A comment must say something the code can't: why, a
   constraint, a non-obvious consequence. No comment that restates a name or a
   signature (`/// Spot.` on `Spot`, `/// Adds a venue.` on `add_venue`). Docs
   are not required on public items: clippy is set up not to ask for them.
7. Open a PR with the template; rebase-merge into `main` (the only merge method
   enabled). Every commit lands on `main` as-is, so keep each one meaningful.
