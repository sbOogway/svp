//! Bybit: spot and USDT linear perpetuals. Public streams only, no API keys.

use nautilus_bybit::{
    common::enums::BybitProductType, config::BybitDataClientConfig,
    factories::BybitDataClientFactory,
};

use super::{DataClientSpec, Market, Pair};

pub(super) fn symbol(market: Market, pair: &Pair) -> Option<String> {
    let (base, quote) = (pair.base(), pair.quote());
    match market {
        Market::Spot => Some(format!("{base}{quote}-SPOT")),
        // USDC perpetuals use another naming scheme (`BTCPERP`), not mapped.
        Market::Futures if quote == "USDT" => Some(format!("{base}{quote}-LINEAR")),
        Market::Futures => None,
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
