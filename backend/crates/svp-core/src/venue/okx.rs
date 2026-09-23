//! OKX: USDT spot and USDT perpetual swaps (`BTC-USD-SWAP` is inverse).
//! Public streams only, no API keys.

use nautilus_okx::{
    common::enums::OKXInstrumentType, config::OKXDataClientConfig, factories::OKXDataClientFactory,
};

use super::{DataClientSpec, Market};

pub(super) fn symbol(market: Market, base: &str) -> String {
    match market {
        Market::Spot => format!("{base}-USDT"),
        Market::Futures => format!("{base}-USDT-SWAP"),
    }
}

pub(super) fn data_client(market: Market) -> DataClientSpec {
    let config = OKXDataClientConfig {
        instrument_types: vec![match market {
            Market::Spot => OKXInstrumentType::Spot,
            Market::Futures => OKXInstrumentType::Swap,
        }],
        ..Default::default()
    };
    DataClientSpec {
        factory: Box::new(OKXDataClientFactory::new()),
        config: Box::new(config),
    }
}
