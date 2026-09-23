//! Hyperliquid: USD perpetuals; spot is not mapped. One client serves every
//! market. Public streams only, no private key.

use nautilus_hyperliquid::{
    config::HyperliquidDataClientConfig, factories::HyperliquidDataClientFactory,
};

use super::{DataClientSpec, Market, Pair};

pub(super) fn symbol(market: Market, pair: &Pair) -> Option<String> {
    match market {
        Market::Futures if pair.quote() == "USD" => Some(format!("{}-USD-PERP", pair.base())),
        Market::Spot | Market::Futures => None,
    }
}

pub(super) fn data_client() -> DataClientSpec {
    DataClientSpec {
        factory: Box::new(HyperliquidDataClientFactory::new()),
        config: Box::new(HyperliquidDataClientConfig::default()),
    }
}
