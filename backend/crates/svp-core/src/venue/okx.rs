//! OKX: one data client per market (spot, swap or futures), all on venue
//! `OKX`. Public streams only, no API keys.

use nautilus_model::identifiers::{ClientId, InstrumentId, Venue};
use nautilus_okx::{
    common::enums::OKXInstrumentType, config::OKXDataClientConfig, factories::OKXDataClientFactory,
};

use super::{DataClientSpec, Feed};

/// OKX market served by a data client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OkxMarket {
    /// Spot (`BTC-USDT.OKX`).
    Spot,
    /// Perpetual swaps (`BTC-USDT-SWAP.OKX`).
    Swap,
    /// Dated futures (`BTC-USD-241220.OKX`).
    Futures,
}

impl OkxMarket {
    const fn instrument_type(self) -> OKXInstrumentType {
        match self {
            Self::Spot => OKXInstrumentType::Spot,
            Self::Swap => OKXInstrumentType::Swap,
            Self::Futures => OKXInstrumentType::Futures,
        }
    }

    const fn client_name(self) -> &'static str {
        match self {
            Self::Spot => "OKX-SPOT",
            Self::Swap => "OKX-SWAP",
            Self::Futures => "OKX-FUTURES",
        }
    }
}

/// An OKX data client for one market.
#[derive(Debug, Clone)]
pub struct OkxFeed {
    market: OkxMarket,
    instrument_ids: Vec<InstrumentId>,
}

impl OkxFeed {
    /// Creates a feed for `instrument_ids` on `market`.
    #[must_use]
    pub fn new(market: OkxMarket, instrument_ids: Vec<InstrumentId>) -> Self {
        Self {
            market,
            instrument_ids,
        }
    }
}

impl Feed for OkxFeed {
    fn client_id(&self) -> ClientId {
        ClientId::from(self.market.client_name())
    }

    fn venue(&self) -> Venue {
        Venue::from("OKX")
    }

    fn instrument_ids(&self) -> &[InstrumentId] {
        &self.instrument_ids
    }

    fn data_client(&self) -> DataClientSpec {
        let config = OKXDataClientConfig {
            instrument_types: vec![self.market.instrument_type()],
            ..Default::default()
        };
        DataClientSpec {
            factory: Box::new(OKXDataClientFactory::new()),
            config: Box::new(config),
        }
    }
}
