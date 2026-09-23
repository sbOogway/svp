use nautilus_kraken::{
    common::enums::KrakenProductType, config::KrakenDataClientConfig,
    factories::KrakenDataClientFactory,
};
use nautilus_model::identifiers::{InstrumentId, Symbol};

use super::{Coin, DataClientSpec, Exchange, Market};

pub(super) struct Kraken;

impl Exchange for Kraken {
    fn symbol(&self, market: Market, coin: Coin) -> Option<Symbol> {
        Some(Symbol::new(match market {
            Market::Spot => format!("{coin}/USD"),
            Market::Futures if coin == Coin::BTC => "PF_XBTUSD".to_string(),
            Market::Futures => format!("PF_{coin}USD"),
        }))
    }

    fn data_client(&self, market: Market, _instrument_ids: &[InstrumentId]) -> DataClientSpec {
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
}
