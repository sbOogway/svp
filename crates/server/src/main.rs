use svp_aggregator::{
    sink::{ChannelSink, LogSink, Sink},
    venue::{Coin, FeedsBuilder, Market, Venue},
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
        .add_venue(Venue::Binance)
        .add_venue(Venue::Bybit)
        .add_venue(Venue::Okx)
        .add_venue(Venue::Kraken)
        .add_venue(Venue::Coinbase)
        .add_venue(Venue::Hyperliquid)
        .add_market(Market::Futures)
        .add_instrument(Coin::BTC)
        .build()?;

    // The transport to the app (M2) will subscribe receivers from `_updates`.
    let (channel, _updates) = ChannelSink::new(4096);
    let sinks: Vec<Box<dyn Sink>> = vec![Box::new(LogSink), Box::new(channel)];
    let mut node = svp_aggregator::node::build(&feeds, sinks)?;
    node.run().await
}
