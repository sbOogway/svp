use nautilus_binance::{
    common::enums::{BinanceEnvironment, BinanceProductType},
    config::{BinanceDataClientConfig, BinanceInstrumentProviderConfig, BinanceSpotMarketDataMode},
    factories::BinanceDataClientFactory,
};
use nautilus_model::identifiers::InstrumentId;

use super::{Coin, DataClientSpec, Exchange, Market};

pub(super) struct Binance;

impl Exchange for Binance {
    fn symbol(&self, market: Market, coin: Coin) -> Option<String> {
        Some(match market {
            Market::Spot => format!("{coin}USDT"),
            Market::Futures => format!("{coin}USDT-PERP"),
        })
    }

    fn data_client(&self, market: Market, instrument_ids: &[InstrumentId]) -> DataClientSpec {
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
}
