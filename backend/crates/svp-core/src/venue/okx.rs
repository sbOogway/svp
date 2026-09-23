//! OKX: spot and USDT/USDC perpetual swaps (`BTC-USD-SWAP` is inverse, so USD
//! is not mapped). Public streams only, no API keys.

use nautilus_okx::{
    common::enums::OKXInstrumentType, config::OKXDataClientConfig, factories::OKXDataClientFactory,
};

use super::{DataClientSpec, Market, Pair};

pub(super) fn symbol(market: Market, pair: &Pair) -> Option<String> {
    let (base, quote) = (pair.base(), pair.quote());
    match market {
        Market::Spot => Some(format!("{base}-{quote}")),
        Market::Futures if matches!(quote, "USDT" | "USDC") => Some(format!("{base}-{quote}-SWAP")),
        Market::Futures => None,
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
