use nautilus_hyperliquid::{
    config::HyperliquidDataClientConfig, factories::HyperliquidDataClientFactory,
};

use super::{DataClientSpec, Market};

pub(super) fn symbol(market: Market, base: &str) -> Option<String> {
    match market {
        Market::Spot => None,
        Market::Futures => Some(format!("{base}-USD-PERP")),
    }
}

pub(super) fn data_client() -> DataClientSpec {
    DataClientSpec {
        factory: Box::new(HyperliquidDataClientFactory::new()),
        config: Box::new(HyperliquidDataClientConfig::default()),
    }
}
