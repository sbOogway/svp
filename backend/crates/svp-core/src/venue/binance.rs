//! Binance: spot and USD-M perpetuals (USDT or USDC margined). Public streams
//! only, no API keys.

use nautilus_binance::{
    common::enums::{BinanceEnvironment, BinanceProductType},
    config::{BinanceDataClientConfig, BinanceInstrumentProviderConfig, BinanceSpotMarketDataMode},
    factories::BinanceDataClientFactory,
};
use nautilus_model::identifiers::InstrumentId;

use super::{DataClientSpec, Market, Pair};

pub(super) fn symbol(market: Market, pair: &Pair) -> Option<String> {
    let (base, quote) = (pair.base(), pair.quote());
    match market {
        Market::Spot => Some(format!("{base}{quote}")),
        Market::Futures if matches!(quote, "USDT" | "USDC") => Some(format!("{base}{quote}-PERP")),
        Market::Futures => None,
    }
}

pub(super) fn data_client(market: Market, instrument_ids: &[InstrumentId]) -> DataClientSpec {
    // Load only the instruments we subscribe to: `load_all` fetches the whole
    // exchange info (~500 symbols) and warns for every non-trading one.
    let config = BinanceDataClientConfig {
        product_type: match market {
            Market::Spot => BinanceProductType::Spot,
            Market::Futures => BinanceProductType::UsdM,
        },
        environment: BinanceEnvironment::Live,
        // SBE (the default) requires Ed25519 keys; ignored off spot.
        spot_market_data_mode: BinanceSpotMarketDataMode::Json,
        instrument_provider: BinanceInstrumentProviderConfig {
            load_all: false,
            load_ids: Some(instrument_ids.iter().map(ToString::to_string).collect()),
            ..Default::default()
        },
        ..Default::default()
    };
    DataClientSpec {
        factory: Box::new(BinanceDataClientFactory::new()),
        config: Box::new(config),
    }
}
