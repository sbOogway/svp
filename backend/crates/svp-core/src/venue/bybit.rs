//! Bybit: one data client per market (spot, linear or inverse), all on venue
//! `BYBIT`. Public streams only, no API keys.

use nautilus_bybit::{
    common::enums::BybitProductType, config::BybitDataClientConfig,
    factories::BybitDataClientFactory,
};
use nautilus_model::identifiers::{ClientId, InstrumentId, Venue};

use super::{DataClientSpec, Feed};

/// Bybit market served by a data client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BybitMarket {
    /// Spot (`BTCUSDT-SPOT.BYBIT`).
    Spot,
    /// USDT/USDC contracts (`BTCUSDT-LINEAR.BYBIT`).
    Linear,
    /// Coin-margined contracts (`BTCUSD-INVERSE.BYBIT`).
    Inverse,
}

impl BybitMarket {
    const fn product_type(self) -> BybitProductType {
        match self {
            Self::Spot => BybitProductType::Spot,
            Self::Linear => BybitProductType::Linear,
            Self::Inverse => BybitProductType::Inverse,
        }
    }

    const fn client_name(self) -> &'static str {
        match self {
            Self::Spot => "BYBIT-SPOT",
            Self::Linear => "BYBIT-LINEAR",
            Self::Inverse => "BYBIT-INVERSE",
        }
    }
}

/// A Bybit data client for one market.
#[derive(Debug, Clone)]
pub struct BybitFeed {
    market: BybitMarket,
    instrument_ids: Vec<InstrumentId>,
}

impl BybitFeed {
    /// Creates a feed for `instrument_ids` on `market`.
    #[must_use]
    pub fn new(market: BybitMarket, instrument_ids: Vec<InstrumentId>) -> Self {
        Self {
            market,
            instrument_ids,
        }
    }
}

impl Feed for BybitFeed {
    fn client_id(&self) -> ClientId {
        ClientId::from(self.market.client_name())
    }

    fn venue(&self) -> Venue {
        Venue::from("BYBIT")
    }

    fn instrument_ids(&self) -> &[InstrumentId] {
        &self.instrument_ids
    }

    fn data_client(&self) -> DataClientSpec {
        let config = BybitDataClientConfig {
            product_types: vec![self.market.product_type()],
            ..Default::default()
        };
        DataClientSpec {
            factory: Box::new(BybitDataClientFactory::new()),
            config: Box::new(config),
        }
    }
}
