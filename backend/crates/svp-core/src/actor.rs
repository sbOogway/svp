use nautilus_common::{
    actor::{DataActor, DataActorCore, data_actor::DataActorConfig},
    nautilus_actor,
};
use nautilus_model::{
    data::TradeTick,
    instruments::{Instrument, InstrumentAny},
    types::Quantity,
};

use crate::venue::Subscription;

/// Exists to prove the node connects and data flows; the aggregation actor
/// (`SvpActor`) supersedes it.
#[derive(Debug)]
pub struct TradeLogger {
    core: DataActorCore,
    subscriptions: Vec<Subscription>,
}

nautilus_actor!(TradeLogger);

impl TradeLogger {
    #[must_use]
    pub fn new(subscriptions: Vec<Subscription>) -> Self {
        Self {
            core: DataActorCore::new(DataActorConfig::default()),
            subscriptions,
        }
    }
}

impl DataActor for TradeLogger {
    fn on_start(&mut self) -> anyhow::Result<()> {
        // Route by client, not by venue: one venue can have several clients
        // (Binance spot and futures), none of them named after the venue.
        for sub in self.subscriptions.clone() {
            // Some clients load only part of their venue's instruments on
            // connect (Coinbase: spot only) and drop trades for the rest.
            self.request_instrument(sub.instrument_id, None, None, Some(sub.client_id), None)?;
            log::info!(
                "subscribing to trades for {} on {}",
                sub.instrument_id,
                sub.client_id
            );
            self.subscribe_trades(sub.instrument_id, Some(sub.client_id), None);
        }
        Ok(())
    }

    fn on_stop(&mut self) -> anyhow::Result<()> {
        for sub in self.subscriptions.clone() {
            self.unsubscribe_trades(sub.instrument_id, Some(sub.client_id), None);
        }
        log::info!("stopped TradeLogger");
        Ok(())
    }

    fn on_trade(&mut self, tick: &TradeTick) -> anyhow::Result<()> {
        let Some(instrument) = self.cache().instrument(&tick.instrument_id) else {
            log::warn!("no instrument for {}, trade dropped", tick.instrument_id);
            return Ok(());
        };
        log::info!(
            "{} {:?} {} @ {} id={} ts_event={}",
            tick.instrument_id,
            tick.aggressor_side,
            size_in_coins(tick, &instrument),
            tick.price,
            tick.trade_id,
            tick.ts_event,
        );
        Ok(())
    }
}

/// A trade's size in coins. Some venues count derivatives in contracts (an
/// OKX or Coinbase BTC perp contract is 0.01 BTC); the instrument's
/// multiplier is the contract size, and 1 where sizes are already in coins.
pub fn size_in_coins(tick: &TradeTick, instrument: &InstrumentAny) -> Quantity {
    tick.size * instrument.multiplier()
}

#[cfg(test)]
mod tests {
    use nautilus_model::{
        enums::AggressorSide,
        identifiers::{InstrumentId, Symbol, TradeId},
        instruments::CryptoPerpetual,
        types::{Currency, Price},
    };

    use super::*;

    fn trade_of(size: &str, multiplier: &str) -> Quantity {
        let instrument_id = InstrumentId::from("BTC-USDT-SWAP.OKX");
        let instrument = CryptoPerpetual::builder()
            .instrument_id(instrument_id)
            .raw_symbol(Symbol::from("BTC-USDT-SWAP"))
            .base_currency(Currency::BTC())
            .quote_currency(Currency::USDT())
            .settlement_currency(Currency::USDT())
            .is_inverse(false)
            .price_precision(1)
            .size_precision(2)
            .price_increment(Price::from("0.1"))
            .size_increment(Quantity::from("0.01"))
            .multiplier(Quantity::from(multiplier))
            .ts_event(0.into())
            .ts_init(0.into())
            .build()
            .unwrap();
        let tick = TradeTick::new(
            instrument_id,
            Price::from("84000.0"),
            Quantity::from(size),
            AggressorSide::Buy,
            TradeId::from("1"),
            0.into(),
            0.into(),
        );
        size_in_coins(&tick, &InstrumentAny::CryptoPerpetual(instrument))
    }

    #[test]
    fn converts_contracts_to_coins() {
        assert_eq!(trade_of("3", "0.01"), Quantity::from("0.03"));
    }

    #[test]
    fn keeps_sizes_already_in_coins() {
        assert_eq!(trade_of("0.25", "1"), Quantity::from("0.25"));
    }
}
