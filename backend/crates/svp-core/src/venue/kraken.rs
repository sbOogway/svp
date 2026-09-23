//! Kraken: spot and USD multi-collateral perpetuals (`PF_`, where BTC is
//! `XBT`). Public streams only, no API keys.

use nautilus_kraken::{
    common::enums::KrakenProductType, config::KrakenDataClientConfig,
    factories::KrakenDataClientFactory,
};

use super::{DataClientSpec, Market, Pair};

pub(super) fn symbol(market: Market, pair: &Pair) -> Option<String> {
    let (base, quote) = (pair.base(), pair.quote());
    match market {
        Market::Spot => Some(format!("{base}/{quote}")),
        Market::Futures if quote == "USD" => {
            let base = if base == "BTC" { "XBT" } else { base };
            Some(format!("PF_{base}USD"))
        }
        Market::Futures => None,
    }
}

pub(super) fn data_client(market: Market) -> DataClientSpec {
    let config = KrakenDataClientConfig {
        product_type: match market {
            Market::Spot => KrakenProductType::Spot,
            Market::Futures => KrakenProductType::Futures,
        },
        ..Default::default()
    };
    DataClientSpec {
        factory: Box::new(KrakenDataClientFactory::new()),
        config: Box::new(config),
    }
}
