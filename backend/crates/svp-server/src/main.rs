//! svp backend binary. Populated in milestones M1 and M2.

use svp_core::venue::{
    FeedsBuilder, binance::BinanceMarket, bybit::BybitMarket, kraken::KrakenMarket, okx::OkxMarket,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Nautilus owns the global `log` logger (the kernel refuses to start
    // otherwise), so the tracing subscriber is installed without the
    // `log` bridge that `fmt().init()` would add.
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "svp starting");

    let feeds = FeedsBuilder::new()
        .binance(BinanceMarket::UsdM, &["BTCUSDT-PERP.BINANCE"])
        .binance(BinanceMarket::Spot, &["BTCUSDT.BINANCE"])
        .bybit(BybitMarket::Linear, &["BTCUSDT-LINEAR.BYBIT"])
        .okx(OkxMarket::Swap, &["BTC-USDT-SWAP.OKX"])
        .kraken(KrakenMarket::Futures, &["PF_XBTUSD.KRAKEN"])
        .hyperliquid(&["BTC-USD-PERP.HYPERLIQUID"])
        .build()?;

    let mut node = svp_core::node::build(&feeds)?;
    node.run().await
}
