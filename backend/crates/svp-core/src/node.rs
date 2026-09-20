//! Construction of the Nautilus [`LiveNode`] that hosts every venue client.

use nautilus_binance::{
    common::enums::{BinanceEnvironment, BinanceProductType},
    config::{BinanceDataClientConfig, BinanceInstrumentProviderConfig},
    factories::BinanceDataClientFactory,
};
use nautilus_common::enums::Environment;
use nautilus_live::node::LiveNode;
use nautilus_model::identifiers::{InstrumentId, TraderId};

use crate::actor::TradeLogger;

/// Builds a node with a Binance USD-M futures data client (public streams, no
/// keys) and a [`TradeLogger`] for `instrument_ids`.
///
/// Building the node initializes Nautilus logging, which registers the global
/// `log` logger: the caller must not have registered another one.
///
/// # Errors
///
/// Returns an error if the node or the client configuration is invalid.
pub fn build(instrument_ids: Vec<InstrumentId>) -> anyhow::Result<LiveNode> {
    // Load only the instruments we subscribe to: `load_all` fetches the whole
    // exchange info (~500 symbols) and warns for every non-trading one.
    let binance = BinanceDataClientConfig {
        product_type: BinanceProductType::UsdM,
        environment: BinanceEnvironment::Live,
        instrument_provider: BinanceInstrumentProviderConfig {
            load_all: false,
            load_ids: Some(instrument_ids.iter().map(ToString::to_string).collect()),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut node = LiveNode::builder(TraderId::from("SVP-001"), Environment::Live)?
        .with_name("svp")
        .with_delay_post_stop_secs(1)
        .add_data_client(
            None,
            Box::new(BinanceDataClientFactory::new()),
            Box::new(binance),
        )?
        .build()?;

    node.add_actor(TradeLogger::new(instrument_ids))?;
    Ok(node)
}
