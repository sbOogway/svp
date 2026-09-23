//! Hyperliquid: a single data client on venue `HYPERLIQUID` serves every
//! market. Public streams only, no private key.

use nautilus_hyperliquid::{
    config::HyperliquidDataClientConfig, factories::HyperliquidDataClientFactory,
};
use nautilus_model::identifiers::{ClientId, InstrumentId, Venue};

use super::{DataClientSpec, Feed};

/// The Hyperliquid data client (`BTC-USD-PERP.HYPERLIQUID`).
#[derive(Debug, Clone)]
pub struct HyperliquidFeed {
    instrument_ids: Vec<InstrumentId>,
}

impl HyperliquidFeed {
    /// Creates a feed for `instrument_ids`.
    #[must_use]
    pub fn new(instrument_ids: Vec<InstrumentId>) -> Self {
        Self { instrument_ids }
    }
}

impl Feed for HyperliquidFeed {
    fn client_id(&self) -> ClientId {
        ClientId::from("HYPERLIQUID")
    }

    fn venue(&self) -> Venue {
        Venue::from("HYPERLIQUID")
    }

    fn instrument_ids(&self) -> &[InstrumentId] {
        &self.instrument_ids
    }

    fn data_client(&self) -> DataClientSpec {
        DataClientSpec {
            factory: Box::new(HyperliquidDataClientFactory::new()),
            config: Box::new(HyperliquidDataClientConfig::default()),
        }
    }
}
