# Contributing

1. Install [prek](https://prek.j178.dev/installation/), then run `./scripts/setup.sh`
   once per clone (installs the hooks; nothing works without them).
2. Pick or open an issue; every PR references one and targets a milestone.
3. Branch from `main`: `<type>/<short-name>` (e.g. `feat/bar-accumulator`).
4. Commits follow [Conventional Commits](https://www.conventionalcommits.org):
   `feat(core): …`, `fix(server): …`, `docs: …`. Scopes: `protocol`, `core`,
   `server`, `frontend`, `infra`, `docs`.
5. `make ci` must pass locally — the `pre-push` hook enforces it. There is no hosted CI.
6. Open a PR with the template; rebase-merge into `main` (the only merge method
   enabled). Every commit lands on `main` as-is, so keep each one meaningful.
