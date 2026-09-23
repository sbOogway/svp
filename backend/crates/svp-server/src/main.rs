use svp_core::venue::{Coin, FeedsBuilder, Market, Venue};
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
        .add_venue(Venue::Binance)
        .add_venue(Venue::Bybit)
        .add_venue(Venue::Okx)
        .add_venue(Venue::Kraken)
        .add_venue(Venue::Hyperliquid)
        .add_market(Market::Futures)
        .add_instrument(Coin::BTC)
        .build()?;

    let mut node = svp_core::node::build(&feeds)?;
    node.run().await
}
