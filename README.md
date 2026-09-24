# svp — sbOogway's volumetric platform

A monolithic crypto market-data platform: a Rust server built on
[NautilusTrader](https://nautilustrader.io) subscribes to the trade streams of
many exchanges, aggregates them into enriched candles (buy/sell volume, delta,
VWAP…), and streams them over a Unix domain socket to a native [Iced](https://iced.rs) app.

## Getting started

Requirements: [rustup](https://rustup.rs) (toolchain pinned in `rust-toolchain.toml`), [prek](https://prek.j178.dev/installation/).

```sh
./scripts/setup.sh     # installs git hooks
make ci-fast           # fmt, clippy
make ci                # + tests, audit  (what pre-push runs)
cargo run              # merges BTC perp trades and books from every venue into BTC-PERP.SVP, logs volume per minute
cargo run -p svp-transport --example tail   # prints what the running server streams
```

The server listens on `$XDG_RUNTIME_DIR/svp.sock`; change it with `--socket PATH`
or `SVP_SOCKET`.

Logging: the node and svp's actors log through Nautilus, configured with
`NAUTILUS_LOG`; the server's startup logs through `tracing`, filtered with `RUST_LOG`.

```sh
# every unified trade and merged book delta, and nothing but svp's own logs
NAUTILUS_LOG="stdout=Debug;log_components_only;svp_aggregator::=Debug;svp_transport::=Debug" cargo run
# everything at debug, Nautilus included (very verbose)
NAUTILUS_LOG="stdout=Debug" cargo run
```

## Licensing

svp is MIT licensed. It depends on NautilusTrader, which is LGPL-3.0; it is used
as an unmodified library dependency.
