//! Data actors that run inside the Nautilus node.

use nautilus_common::{
    actor::{DataActor, DataActorCore, data_actor::DataActorConfig},
    nautilus_actor,
};
use nautilus_model::data::TradeTick;

use crate::venue::Subscription;

/// Minimal actor: subscribes to the trade stream of each subscription and logs
/// every trade. Exists to prove the node connects and data flows; the
/// aggregation actor (`SvpActor`) supersedes it.
#[derive(Debug)]
pub struct TradeLogger {
    core: DataActorCore,
    subscriptions: Vec<Subscription>,
    n_trades: u64,
}

nautilus_actor!(TradeLogger);

impl TradeLogger {
    /// Creates a logger for the given subscriptions.
    #[must_use]
    pub fn new(subscriptions: Vec<Subscription>) -> Self {
        Self {
            core: DataActorCore::new(DataActorConfig::default()),
            subscriptions,
            n_trades: 0,
        }
    }
}

impl DataActor for TradeLogger {
    fn on_start(&mut self) -> anyhow::Result<()> {
        // Route by client, not by venue: one venue can have several clients
        // (Binance spot and futures), none of them named after the venue.
        for sub in self.subscriptions.clone() {
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
