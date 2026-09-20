# Architecture

Decisions taken on 2026-09-19, before any product code. Each section states the
decision, the alternatives considered, and why. Update this file when a decision
changes; it is the source of truth, not the chat that produced it.

## Goal

Ingest trades from as many crypto venues as possible, aggregate them locally into
candles enriched with volumetric fields, and stream them to a browser chart.
Volumetric analysis (buy/sell volume, delta, CVD, later volume profile and
footprint) is the core feature; order execution is out of scope for now.

## Backend: pure Rust on NautilusTrader

- `nautilus-live` (`LiveNode`), `nautilus-model`, `nautilus-common` and the venue
  adapter crates, **pinned to an exact version** (the Rust API changes between
  the ~biweekly releases; 0.64.0 requires Rust 1.98.1, see `rust-toolchain.toml`).
- One `LiveNode` holds one data client per venue (per product type where the
  adapter requires it: Binance, Kraken). Every crypto adapter in the Rust tree
  ships a `DataClientFactory` and implements `subscribe_trades`; public trade
  streams need no API keys.
- One trade subscription per instrument per venue. All timeframes are built
  **locally** by Nautilus' `DataEngine` from that stream (`BarType`
  `…-LAST-INTERNAL`, `TimeBarAggregator` etc.). No kline subscriptions for live
  data; exchange klines are only used for historical backfill (M5).
- `SvpActor` implements `DataActor`: subscribes in `on_start`, keeps a per-open-bar
  accumulator per timeframe fed by `on_trade` (aggressor side → buy/sell volume,
  trade count, VWAP) and reconciles it with the OHLCV bar delivered by `on_bar`.
- Volumes are normalized to base-asset quantity using instrument metadata
  (linear vs inverse contracts differ across venues).
- **Logging**: the Nautilus kernel registers the global `log` logger and refuses
  to start if another one is present, so the binary installs its `tracing`
  subscriber *without* the `log` bridge (`set_global_default`, not
  `fmt().init()`). Code that runs inside the node (actors) uses `log`; the web
  server uses `tracing`. Filters: `NAUTILUS_LOG` and `RUST_LOG` respectively.
- **Trade streams are venue-specific**: Binance futures deliver `aggTrade`
  (fills of one taker order at one price merged), so `n_trades` counts
  aggregated trades there, not individual fills. Recorded per venue as each
  adapter lands (M1 #8).
- **Threading**: the Nautilus node is single-threaded (`Rc<RefCell>`, `!Send`).
  The actor does no I/O; it pushes `BarEvent`s into a `tokio::sync::broadcast`
  channel. The web server consumes that channel on the multi-threaded runtime.
  Alternative considered: one node per venue for fault isolation — deferred,
  the venue list is config-driven so this is a deployment change later.

## Transport: REST for history, WebSocket for live

- `axum` + `tower-http`. `GET /api/instruments`, `GET /api/bars`, `GET /healthz`;
  `/ws` for streaming.
- Protocol: a single serde tagged enum pair (`ClientMsg`/`ServerMsg`) in
  `svp-protocol`; TypeScript types generated with `ts-rs` and checked in.
  JSON encoding; MessagePack is a one-line change if raw trades ever stream.
- `StreamId` = `venue:symbol:timeframe`. Config defines *canonical asset groups*
  (e.g. `btc-perp` → all venue ids) for the UI.
- On subscribe the server sends a snapshot from a per-stream ring buffer, then
  updates. Open-bar updates are coalesced (~200 ms); closed bars are sent
  immediately. `broadcast::Lagged` → resend snapshot; the node thread is never
  blocked by a slow client.
- Security regardless of deployment: `Origin` allowlist on the WS handshake
  (CORS does not apply to WebSockets), `CorsLayer` on REST, per-connection
  stream cap, frame-size and idle limits. Cloudflare Access JWT verification is
  a feature flag, off for local use.

## Frontend: Vite + React + TypeScript + KLineChart

- **KLineChart** chosen over TradingView Lightweight Charts for this use case:
  indicators are first-class (`registerIndicator` with `calc`/`figures`/`draw`,
  tooltip for free), timestamps are milliseconds (non-time bars from Nautilus
  work without a custom axis), 30+ built-in indicators and drawing tools.
  Lightweight Charts remains plan B behind a thin `ChartAdapter` interface
  (better ecosystem and TradingView backing; volume profile/heatmap have
  official examples there).
- Rejected: ApexCharts (SVG, not finance-specific, revenue-based license);
  TradingView Advanced Charts (proprietary, pull-based datafeed); a Rust/WASM
  frontend (the chart is JS canvas either way, so WASM buys nothing on
  rendering; the real benefit — shared types — comes from `ts-rs`).
- Indicators that need trades (volume, buy/sell, delta, VWAP, profile) are
  computed in Rust and shipped on the bar. Indicators derived from candles
  (EMA, RSI…) are computed in the browser.

## Deployment

- The binary serves `frontend/dist` when present (`ServeDir`), so one
  `cargo run` is a complete local install.
- Public deployment: backend on own infrastructure behind **Cloudflare Tunnel**
  (no open ports) with **Cloudflare Access** in front (identity-based auth;
  the backend verifies `Cf-Access-Jwt-Assertion`). Frontend on **GitHub
  Pages**, "deploy from branch" (`gh-pages`), published by `scripts/deploy-pages.sh`
  from the `post-merge` hook when a pull of `main` changed `frontend/`. HTTPS/WSS is mandatory because
  Pages is HTTPS (mixed content).
- Restricting the backend to the Pages origin only protects against other
  websites using a visitor's browser; it does not authenticate people — Access
  does.

## Workflow

- GitHub: issues, milestones, PRs into a protected `main`, Conventional Commits.
- **No GitHub Actions.** All checks run locally through versioned git hooks
  (`.githooks/`, enabled by `core.hooksPath`): `pre-commit` runs the fast checks
  on touched areas, `commit-msg` enforces Conventional Commits, `pre-push` runs the full `make ci`, and `post-merge` publishes Pages when a
  pull of `main` changed `frontend/` (main is protected: it only moves by merging PRs). `make ci-clean` builds
  from scratch before a release.
- Known trade-offs, accepted for a solo project: hooks can be bypassed with
  `--no-verify` and GitHub cannot require them; nothing runs unattended
  (scheduled audits, clean-machine builds). Revisit if a second contributor joins.

## Milestones

M0 Foundations · M1 Multi-venue trade-driven aggregation · M2 Streaming core ·
M3 Chart MVP · M4 Assets, bar types, remaining venues · M5 Hardening & deploy.
Details and acceptance criteria live on the GitHub milestones.
