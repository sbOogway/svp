# svp — sbOogway's volumetric platform

A monolithic crypto market-data platform: a Rust server built on
[NautilusTrader](https://nautilustrader.io) subscribes to the trade streams of
many exchanges, aggregates them into enriched candles (buy/sell volume, delta,
VWAP…), and streams them over WebSocket to a native [Iced](https://iced.rs) app.

Status: **M1 — multi-venue trade-driven aggregation**. See the [milestones](https://github.com/sbOogway/svp/milestones)
and the [architecture decisions](https://github.com/sbOogway/svp/wiki/Architecture) in the wiki.

## Layout

```
crates/protocol   wire types shared by the server and the app
crates/core       Nautilus live node, aggregation actor, broadcast bridge
crates/server     axum REST + WebSocket server, binary `svp`
scripts/          setup and git hooks
prek.toml         git hooks = local CI (there is no hosted CI, by design)
```

## Getting started

Requirements: [rustup](https://rustup.rs) (toolchain pinned in `rust-toolchain.toml`), [prek](https://prek.j178.dev/installation/).

```sh
./scripts/setup.sh     # installs git hooks
make ci-fast           # fmt, clippy
make ci                # + tests, audit  (what pre-push runs)
cargo run              # merges BTC perp trades and books from every venue into BTC-PERP.SVP, logs volume per minute
```

Logging: Nautilus components log through the `log` crate, configured with
`NAUTILUS_LOG` (e.g. `NAUTILUS_LOG="stdout=Debug"`); svp's own code logs through
`tracing`, filtered with `RUST_LOG`.

## Licensing

svp is MIT licensed. It depends on NautilusTrader, which is LGPL-3.0; it is used
as an unmodified library dependency.
