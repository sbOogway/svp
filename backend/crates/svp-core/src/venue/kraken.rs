use nautilus_kraken::{
    common::enums::KrakenProductType, config::KrakenDataClientConfig,
    factories::KrakenDataClientFactory,
};

use super::{DataClientSpec, Market};

pub(super) fn symbol(market: Market, base: &str) -> String {
    match market {
        Market::SPOT => format!("{base}/USD"),
        Market::FUTURES => {
            let base = if base == "BTC" { "XBT" } else { base };
            format!("PF_{base}USD")
        }
    }
}

pub(super) fn data_client(market: Market) -> DataClientSpec {
    let config = KrakenDataClientConfig {
        product_type: match market {
            Market::SPOT => KrakenProductType::Spot,
            Market::FUTURES => KrakenProductType::Futures,
        },
        ..Default::default()
    };
    DataClientSpec {
        factory: Box::new(KrakenDataClientFactory::new()),
        config: Box::new(config),
    }
}
