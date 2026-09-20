//! Construction of the Nautilus [`LiveNode`] that hosts every venue client.

use anyhow::{Context, bail};
use nautilus_binance::{
    common::enums::{BinanceEnvironment, BinanceProductType},
    config::{BinanceDataClientConfig, BinanceInstrumentProviderConfig},
    factories::BinanceDataClientFactory,
};
use nautilus_common::enums::Environment;
use nautilus_live::node::{LiveNode, LiveNodeBuilder};

use crate::{
    actor::TradeLogger,
    config::{Adapter, Config, InstrumentConfig, VenueConfig},
};

const TRADER_ID: &str = "SVP-001";

/// Builds a node with one data client per enabled venue in `config` and a
/// [`TradeLogger`] for every instrument bound to those venues.
///
/// Building the node initializes Nautilus logging, which registers the global
/// `log` logger: the caller must not have registered another one.
///
/// # Errors
///
/// Returns an error if a venue uses an adapter that is not implemented yet,
/// its configuration is invalid, or the node cannot be built.
pub fn build(config: &Config) -> anyhow::Result<LiveNode> {
    let mut builder = LiveNode::builder(TRADER_ID.into(), Environment::Live)?
        .with_name("svp")
        .with_delay_post_stop_secs(1);

    for venue in config.enabled_venues() {
        builder = add_venue(builder, config, venue)
            .with_context(|| format!("configuring venue {:?}", venue.id))?;
    }

    let mut node = builder.build()?;
    let instrument_ids = config
        .active_instruments()
        .map(InstrumentConfig::instrument_id)
        .collect();
    node.add_actor(TradeLogger::new(instrument_ids))?;
    Ok(node)
}

fn add_venue(
    builder: LiveNodeBuilder,
    config: &Config,
    venue: &VenueConfig,
) -> anyhow::Result<LiveNodeBuilder> {
    // Every client loads only the instruments it subscribes to: loading a
    // whole exchange info is slow and warns for every non-trading symbol.
    let instrument_ids: Vec<String> = config
        .instruments_for(&venue.id)
        .map(|i| i.id.clone())
        .collect();
    if instrument_ids.is_empty() {
        log::warn!("venue {:?} is enabled but has no instruments", venue.id);
    }

    match venue.adapter {
        Adapter::Binance => {
            let product_type = match venue.product_type.as_deref() {
                Some("spot") => BinanceProductType::Spot,
                Some("usdm") => BinanceProductType::UsdM,
                Some("coinm") => BinanceProductType::CoinM,
                other => {
                    bail!("binance product_type must be one of spot, usdm, coinm; got {other:?}")
                }
            };
            let config = BinanceDataClientConfig {
                product_type,
                environment: BinanceEnvironment::Live,
                instrument_provider: BinanceInstrumentProviderConfig {
                    load_all: false,
                    load_ids: Some(instrument_ids),
                    ..Default::default()
                },
                ..Default::default()
            };
            builder.add_data_client(
                Some(venue.id.clone()),
                Box::new(BinanceDataClientFactory::new()),
                Box::new(config),
            )
        }
        other => bail!("adapter {other:?} is not implemented yet (M1 #8 / #12)"),
    }
}
