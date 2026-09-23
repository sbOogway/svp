use nautilus_hyperliquid::{
    config::HyperliquidDataClientConfig, factories::HyperliquidDataClientFactory,
};
use nautilus_model::identifiers::{InstrumentId, Symbol};

use super::{Coin, DataClientSpec, Exchange, Market};

pub(super) struct Hyperliquid;

impl Exchange for Hyperliquid {
    fn symbol(&self, market: Market, coin: Coin) -> Option<Symbol> {
        match market {
            Market::Spot => None,
            Market::Futures => Some(Symbol::new(format!("{coin}-USD-PERP"))),
        }
    }

    // One client serves every market.
    fn data_client(&self, _market: Market, _instrument_ids: &[InstrumentId]) -> DataClientSpec {
        DataClientSpec {
            factory: Box::new(HyperliquidDataClientFactory::new()),
            config: Box::new(HyperliquidDataClientConfig::default()),
        }
    }
}
