use nautilus_binance::{
    common::enums::{BinanceEnvironment, BinanceProductType},
    config::{BinanceDataClientConfig, BinanceInstrumentProviderConfig, BinanceSpotMarketDataMode},
    factories::BinanceDataClientFactory,
};
use nautilus_model::identifiers::InstrumentId;

use super::{DataClientSpec, Market};

pub(super) fn symbol(market: Market, base: &str) -> String {
    match market {
        Market::SPOT => format!("{base}USDT"),
        Market::FUTURES => format!("{base}USDT-PERP"),
    }
}

pub(super) fn data_client(market: Market, instrument_ids: &[InstrumentId]) -> DataClientSpec {
    // Load only the instruments we subscribe to: `load_all` fetches the whole
    // exchange info (~500 symbols) and warns for every non-trading one.
    let config = BinanceDataClientConfig {
        product_type: match market {
            Market::SPOT => BinanceProductType::Spot,
            Market::FUTURES => BinanceProductType::UsdM,
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
