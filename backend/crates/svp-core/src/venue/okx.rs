use nautilus_model::identifiers::InstrumentId;
use nautilus_okx::{
    common::enums::OKXInstrumentType, config::OKXDataClientConfig, factories::OKXDataClientFactory,
};

use super::{Coin, DataClientSpec, Exchange, Market};

pub(super) struct Okx;

impl Exchange for Okx {
    // USDT, not USD: `BTC-USD-SWAP` is an inverse contract.
    fn symbol(&self, market: Market, coin: Coin) -> Option<String> {
        Some(match market {
            Market::Spot => format!("{coin}-USDT"),
            Market::Futures => format!("{coin}-USDT-SWAP"),
        })
    }

    fn data_client(&self, market: Market, _instrument_ids: &[InstrumentId]) -> DataClientSpec {
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
}
