use nautilus_coinbase::{config::CoinbaseDataClientConfig, factories::CoinbaseDataClientFactory};
use nautilus_model::identifiers::{InstrumentId, Symbol};

use super::{Coin, DataClientSpec, Exchange, Market};

pub(super) struct Coinbase;

impl Exchange for Coinbase {
    // Futures are the US perp-style CDE contracts: funded like perpetuals,
    // with a nominal expiry years away and a product code per coin. Coinbase
    // lists no TRX.
    fn symbol(&self, market: Market, coin: Coin) -> Option<Symbol> {
        if coin == Coin::TRX {
            return None;
        }
        Some(Symbol::new(match market {
            Market::Spot => format!("{coin}-USD"),
            Market::Futures => format!("{}-20DEC30-CDE", perp_code(coin)),
        }))
    }

    // One client serves every market. It loads only spot instruments on
    // connect, so futures depend on the actor requesting each instrument.
    fn data_client(&self, _market: Market, _instrument_ids: &[InstrumentId]) -> DataClientSpec {
        DataClientSpec {
            factory: Box::new(CoinbaseDataClientFactory::new()),
            config: Box::new(CoinbaseDataClientConfig::default()),
        }
    }
}

fn perp_code(coin: Coin) -> &'static str {
    match coin {
        Coin::BTC => "BIP",
        Coin::ETH => "ETP",
        Coin::SOL => "SLP",
        Coin::XRP => "XPP",
        Coin::DOGE => "DOP",
        Coin::BNB => "BNB",
        Coin::ADA => "ADP",
        Coin::AVAX => "AVP",
        Coin::LINK => "LNP",
        Coin::LTC => "LCP",
        Coin::DOT => "POP",
        Coin::SUI => "SUP",
        Coin::BCH => "BCP",
        Coin::TRX => unreachable!("Coinbase lists no TRX"),
    }
}
