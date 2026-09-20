//! svp backend binary. Populated in milestones M1 and M2.

use nautilus_model::identifiers::InstrumentId;
use tracing_subscriber::EnvFilter;

const DEFAULT_INSTRUMENT_ID: &str = "BTCUSDT-PERP.BINANCE";

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

    let instrument_id =
        std::env::var("SVP_INSTRUMENT_ID").unwrap_or_else(|_| DEFAULT_INSTRUMENT_ID.to_string());
    let instrument_ids = vec![InstrumentId::from(instrument_id.as_str())];

    let mut node = svp_core::node::build(instrument_ids)?;
    node.run().await
}
