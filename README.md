# svp — sbOogway's volumetric platform

A monolithic crypto market-data platform: a Rust backend built on
[NautilusTrader](https://nautilustrader.io) subscribes to the trade streams of
many exchanges, aggregates them into enriched candles (buy/sell volume, delta,
VWAP…), and streams them over WebSocket to a TypeScript frontend built on
[KLineChart](https://klinecharts.com).

Status: **M0 — foundations**. See the [milestones](https://github.com/sbOogway/svp/milestones)
and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Layout

```
backend/    Cargo workspace
  crates/svp-protocol   wire types shared with the frontend (TS generated via ts-rs)
  crates/svp-core       Nautilus live node, aggregation actor, broadcast bridge
  crates/svp-server     axum REST + WebSocket server, serves the frontend, binary `svp`
frontend/   Vite + React + TypeScript, KLineChart
docs/       architecture and runbooks
scripts/    setup and deploy
.githooks/  local CI (there is no hosted CI, by design)
```

## Getting started

Requirements: [rustup](https://rustup.rs) (toolchain pinned in `rust-toolchain.toml`), Node ≥ 22.

```sh
./scripts/setup.sh     # installs git hooks + frontend deps
make ci-fast           # fmt, clippy, typecheck, lint  (what pre-commit runs)
make ci                # + tests, audit, release build  (what pre-push runs)
make deploy-pages      # publish frontend to GitHub Pages (post-merge does this on main)
cd backend && cargo run
cd frontend && npm run dev
```

## Licensing

svp is MIT licensed. It depends on NautilusTrader, which is LGPL-3.0; it is used
as an unmodified library dependency.
