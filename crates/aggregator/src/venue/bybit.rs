use std::num::NonZeroUsize;

use nautilus_bybit::{
    common::enums::BybitProductType, config::BybitDataClientConfig,
    factories::BybitDataClientFactory,
};
use nautilus_model::identifiers::{InstrumentId, Symbol};

use super::{Coin, DataClientSpec, Exchange, Market};

pub(super) struct Bybit;

impl Exchange for Bybit {
    fn symbol(&self, market: Market, coin: Coin) -> Option<Symbol> {
        Some(Symbol::new(match market {
            Market::Spot => format!("{coin}USDT-SPOT"),
            Market::Futures => format!("{coin}USDT-LINEAR"),
        }))
    }

    fn data_client(&self, market: Market, _instrument_ids: &[InstrumentId]) -> DataClientSpec {
        let config = BybitDataClientConfig {
            product_types: vec![match market {
                Market::Spot => BybitProductType::Spot,
                Market::Futures => BybitProductType::Linear,
            }],
            ..Default::default()
        };
        DataClientSpec {
            factory: Box::new(BybitDataClientFactory::new()),
            config: Box::new(config),
        }
    }

    fn book_depth(&self, _market: Market) -> Option<NonZeroUsize> {
        NonZeroUsize::new(200)
    }
}
