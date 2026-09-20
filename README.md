# svp — sbOogway's volumetric platform

A monolithic crypto market-data platform: a Rust backend built on
[NautilusTrader](https://nautilustrader.io) subscribes to the trade streams of
many exchanges, aggregates them into enriched candles (buy/sell volume, delta,
VWAP…), and streams them over WebSocket to a TypeScript frontend built on
[KLineChart](https://klinecharts.com).

Status: **M1 — multi-venue trade-driven aggregation**. See the [milestones](https://github.com/sbOogway/svp/milestones)
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
cd backend && cargo run   # subscribes to every instrument in config/ and logs trades
cd frontend && npm run dev
```

Configuration: `backend/config/default.toml` is committed and documents every
section (`[server]`, `[bars]`, `[[venue]]`, `[[instrument]]`, `[[asset]]`).
Override per deployment with `backend/config/local.toml` (gitignored) or
`SVP__SECTION__KEY` environment variables; point elsewhere with
`SVP_CONFIG_DIR`. Invalid config fails at startup with the offending entry.

Logging: Nautilus components log through the `log` crate, configured with
`NAUTILUS_LOG` (e.g. `NAUTILUS_LOG="stdout=Debug"`); svp's own code logs through
`tracing`, filtered with `RUST_LOG`.

## Licensing

svp is MIT licensed. It depends on NautilusTrader, which is LGPL-3.0; it is used
as an unmodified library dependency.
