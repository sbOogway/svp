use nautilus_bybit::{
    common::enums::BybitProductType, config::BybitDataClientConfig,
    factories::BybitDataClientFactory,
};

use super::{DataClientSpec, Market};

pub(super) fn symbol(market: Market, base: &str) -> String {
    match market {
        Market::SPOT => format!("{base}USDT-SPOT"),
        Market::FUTURES => format!("{base}USDT-LINEAR"),
    }
}

pub(super) fn data_client(market: Market) -> DataClientSpec {
    let config = BybitDataClientConfig {
        product_types: vec![match market {
            Market::SPOT => BybitProductType::Spot,
            Market::FUTURES => BybitProductType::Linear,
        }],
        ..Default::default()
    };
    DataClientSpec {
        factory: Box::new(BybitDataClientFactory::new()),
        config: Box::new(config),
    }
}
