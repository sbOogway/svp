//! Data actors that run inside the Nautilus node.

use nautilus_common::{
    actor::{DataActor, DataActorCore, data_actor::DataActorConfig},
    nautilus_actor,
};
use nautilus_model::{data::TradeTick, identifiers::InstrumentId};

/// Minimal actor: subscribes to the trade stream of each instrument and logs
/// every trade. Exists to prove the node connects and data flows; the
/// aggregation actor (`SvpActor`) supersedes it.
#[derive(Debug)]
pub struct TradeLogger {
    core: DataActorCore,
    instrument_ids: Vec<InstrumentId>,
    n_trades: u64,
}

nautilus_actor!(TradeLogger);

impl TradeLogger {
    /// Creates a logger for the given instruments.
    #[must_use]
    pub fn new(instrument_ids: Vec<InstrumentId>) -> Self {
        Self {
            core: DataActorCore::new(DataActorConfig::default()),
            instrument_ids,
            n_trades: 0,
        }
    }
}

impl DataActor for TradeLogger {
    fn on_start(&mut self) -> anyhow::Result<()> {
        // `client_id = None`: the data engine routes by the instrument's venue,
        // which matches the client name the factory registered ("BINANCE").
        for id in self.instrument_ids.clone() {
            log::info!("subscribing to trades for {id}");
            self.subscribe_trades(id, None, None);
        }
        Ok(())
    }

    fn on_stop(&mut self) -> anyhow::Result<()> {
        for id in self.instrument_ids.clone() {
            self.unsubscribe_trades(id, None, None);
        }
        log::info!("stopped after {} trades", self.n_trades);
        Ok(())
    }

    fn on_trade(&mut self, tick: &TradeTick) -> anyhow::Result<()> {
        self.n_trades += 1;
        log::info!(
            "{} {:?} {} @ {} id={} ts_event={}",
            tick.instrument_id,
            tick.aggressor_side,
            tick.size,
            tick.price,
            tick.trade_id,
            tick.ts_event,
        );
        Ok(())
    }
}
