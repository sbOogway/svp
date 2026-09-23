//! Kraken: one data client per market (spot or futures), both on venue
//! `KRAKEN`. Public streams only, no API keys.

use nautilus_kraken::{
    common::enums::KrakenProductType, config::KrakenDataClientConfig,
    factories::KrakenDataClientFactory,
};
use nautilus_model::identifiers::{ClientId, InstrumentId, Venue};

use super::{DataClientSpec, Feed};

/// Kraken market served by a data client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KrakenMarket {
    /// Spot (`BTC/USD.KRAKEN`).
    Spot,
    /// Futures (`PF_XBTUSD.KRAKEN`).
    Futures,
}

impl KrakenMarket {
    const fn product_type(self) -> KrakenProductType {
        match self {
            Self::Spot => KrakenProductType::Spot,
            Self::Futures => KrakenProductType::Futures,
        }
    }

    const fn client_name(self) -> &'static str {
        match self {
            Self::Spot => "KRAKEN-SPOT",
            Self::Futures => "KRAKEN-FUTURES",
        }
    }
}

/// A Kraken data client for one market.
#[derive(Debug, Clone)]
pub struct KrakenFeed {
    market: KrakenMarket,
    instrument_ids: Vec<InstrumentId>,
}

impl KrakenFeed {
    /// Creates a feed for `instrument_ids` on `market`.
    #[must_use]
    pub fn new(market: KrakenMarket, instrument_ids: Vec<InstrumentId>) -> Self {
        Self {
            market,
            instrument_ids,
        }
    }
}

impl Feed for KrakenFeed {
    fn client_id(&self) -> ClientId {
        ClientId::from(self.market.client_name())
    }

    fn venue(&self) -> Venue {
        Venue::from("KRAKEN")
    }

    fn instrument_ids(&self) -> &[InstrumentId] {
        &self.instrument_ids
    }

    fn data_client(&self) -> DataClientSpec {
        let config = KrakenDataClientConfig {
            product_type: self.market.product_type(),
            ..Default::default()
        };
        DataClientSpec {
            factory: Box::new(KrakenDataClientFactory::new()),
            config: Box::new(config),
        }
    }
}
