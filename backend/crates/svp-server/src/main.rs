//! svp backend binary. Populated in milestones M1 and M2.

use svp_core::config::Config;
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

    let config = Config::load()?;
    tracing::info!(
        venues = ?config.enabled_venues().map(|v| v.id.as_str()).collect::<Vec<_>>(),
        instruments = config.active_instruments().count(),
        timeframes = ?config.bars.timeframes.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "configuration loaded"
    );

    let mut node = svp_core::node::build(&config)?;
    node.run().await
}
