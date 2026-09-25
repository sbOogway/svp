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
cargo run --bin svp-server        # merges BTC perp trades and books from every venue into BTC-PERP.SVP, logs volume per minute
cargo run --bin svp-mock-client   # prints what the running server streams
```

The server listens on `$XDG_RUNTIME_DIR/svp.sock`; change it with `--socket PATH`
or `SVP_SOCKET`.

### The app

```sh
cargo run -p svp-client --features app --bin svp-app   # the dashboard, fed by the running server
```

It takes the same `--socket PATH` / `SVP_SOCKET` as the server. Start it before
or after the server: it keeps reconnecting until one answers.

Building needs no system libraries. At run time, on Linux, Iced loads a Wayland
(`libwayland-client`, `libxkbcommon`) or X11 (`libX11`, `libXcursor`,
`libxkbcommon-x11`) client library, and a Vulkan (`libvulkan`) or OpenGL
(`libEGL`) driver; a desktop install has them, and Mesa's drivers are enough.

Logging: the node and svp's actors log through Nautilus, configured with
`NAUTILUS_LOG`; the server's startup logs through `tracing`, filtered with `RUST_LOG`.

```sh
# every unified trade and merged book delta, and nothing but svp's own logs
NAUTILUS_LOG="stdout=Debug;log_components_only;svp_server::aggregator::=Debug" cargo run --bin svp-server
# everything at debug, Nautilus included (very verbose)
NAUTILUS_LOG="stdout=Debug" cargo run --bin svp-server
```

## Licensing

svp is licensed under the GNU General Public License v3.0 only
(`GPL-3.0-only`); see [LICENSE](LICENSE).

Parts of the frontend and protocol code are copied or adapted from
[flowsurface](https://github.com/flowsurface-rs/flowsurface) (GPL-3.0-or-later),
by the flowsurface contributors. Each such file says so in its header.

svp depends on NautilusTrader, which is LGPL-3.0; it is used as an unmodified
library dependency.
