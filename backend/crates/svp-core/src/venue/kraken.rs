//! Kraken: USD spot and USD multi-collateral perpetuals (`PF_`, where BTC is
//! `XBT`). Public streams only, no API keys.

use nautilus_kraken::{
    common::enums::KrakenProductType, config::KrakenDataClientConfig,
    factories::KrakenDataClientFactory,
};

use super::{DataClientSpec, Market};

pub(super) fn symbol(market: Market, base: &str) -> String {
    match market {
        Market::Spot => format!("{base}/USD"),
        Market::Futures => {
            let base = if base == "BTC" { "XBT" } else { base };
            format!("PF_{base}USD")
        }
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
