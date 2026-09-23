use nautilus_bybit::{
    common::enums::BybitProductType, config::BybitDataClientConfig,
    factories::BybitDataClientFactory,
};
use nautilus_model::identifiers::InstrumentId;

use super::{Coin, DataClientSpec, Exchange, Market};

pub(super) struct Bybit;

impl Exchange for Bybit {
    fn symbol(&self, market: Market, coin: Coin) -> Option<String> {
        Some(match market {
            Market::Spot => format!("{coin}USDT-SPOT"),
            Market::Futures => format!("{coin}USDT-LINEAR"),
        })
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
}
