//! Bybit: USDT spot and USDT linear perpetuals. Public streams only, no API
//! keys.

use nautilus_bybit::{
    common::enums::BybitProductType, config::BybitDataClientConfig,
    factories::BybitDataClientFactory,
};

use super::{DataClientSpec, Market};

pub(super) fn symbol(market: Market, base: &str) -> String {
    match market {
        Market::Spot => format!("{base}USDT-SPOT"),
        Market::Futures => format!("{base}USDT-LINEAR"),
    }
}

pub(super) fn data_client(market: Market) -> DataClientSpec {
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
