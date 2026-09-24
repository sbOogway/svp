use nautilus_common::{enums::Environment, logging::config::LoggerConfig};
use nautilus_live::node::LiveNode;
use nautilus_model::identifiers::TraderId;
use svp_transport::sink::Sink;

use crate::{
    actor::VolumeLogger,
    publisher::Publisher,
    unified::{self, SvpDataClientConfig, SvpDataClientFactory, Unifier},
    venue::{self, DataClientSpec, Feed, Market, Venue},
};

/// Building the node initializes Nautilus logging, which registers the global
/// `log` logger: the caller must not have registered another one.
pub fn build(feeds: &[Feed], sinks: Vec<Box<dyn Sink>>) -> anyhow::Result<LiveNode> {
    let mut builder = LiveNode::builder(TraderId::from("SVP-001"), Environment::Live)?
        .with_name("svp")
        .with_delay_post_stop_secs(1);
    // The node ignores `NAUTILUS_LOG` unless it is passed in.
    if std::env::var_os("NAUTILUS_LOG").is_some() {
        builder = builder.with_logging(LoggerConfig::from_env()?);
    }

    let mut builder_clients = Vec::new();
    for feed in feeds {
        let DataClientSpec { factory, config } = feed.data_client();
        builder = builder.add_data_client(Some(feed.client_id().to_string()), factory, config)?;
    }

    // USD rates of the stablecoins venues quote in.
    for source in unified::rate_sources() {
        let has_client = feeds.iter().any(|f| f.client_id() == source.client_id)
            || builder_clients.contains(&source.client_id);
        if !has_client {
            let DataClientSpec { factory, config } =
                venue::data_client(Venue::Kraken, Market::Spot, &[source.instrument_id]);
            builder =
                builder.add_data_client(Some(source.client_id.to_string()), factory, config)?;
            builder_clients.push(source.client_id);
        }
    }

    builder = builder.add_data_client(
        Some(unified::VENUE.to_string()),
        Box::new(SvpDataClientFactory),
        Box::new(SvpDataClientConfig),
    )?;

    let mut node = builder.build()?;
    let unified = unified::unify(&venue::subscriptions(feeds));
    let unified_ids = unified.iter().map(|u| u.instrument_id).collect();
    node.add_actor(Unifier::new(unified.clone()))?;
    node.add_actor(VolumeLogger::new(unified))?;
    node.add_actor(Publisher::new(unified_ids, sinks))?;
    Ok(node)
}
