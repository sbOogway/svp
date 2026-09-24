use std::collections::{HashMap, HashSet};

use nautilus_common::{
    actor::{DataActor, DataActorConfig, DataActorCore, DataActorNative},
    msgbus::{self, switchboard},
    nautilus_actor,
    timer::TimeEvent,
};
use nautilus_core::DurationNanos;
use nautilus_model::{
    data::{OrderBookDeltas, TradeTick},
    enums::{BookType, OrderSide},
    identifiers::{ActorId, InstrumentId},
    instruments::{Instrument, InstrumentAny},
};

use super::{MergedBook, Unified, build_instrument, unify_trade};
use crate::venue::Subscription;

const BUILD_TIMER: &str = "unifier-build";
/// How long to wait for every venue instrument before building a unified
/// instrument from those that arrived, so one failing venue doesn't hold
/// back the others.
const BUILD_TIMEOUT: DurationNanos = DurationNanos::from_secs(30);

/// Republishes the trades and books of each unified instrument's venues as
/// the unified instrument's own.
///
/// The unified instrument is built once its venue instruments are loaded, as
/// its tick is the finest of theirs; venue data is subscribed from then on.
#[derive(Debug)]
pub struct Unifier {
    core: DataActorCore,
    unified: Vec<Unified>,
    unified_of: HashMap<InstrumentId, InstrumentId>,
    venue_instruments: HashMap<InstrumentId, InstrumentAny>,
    built: HashMap<InstrumentId, Built>,
    subscribed: HashSet<Subscription>,
}

#[derive(Debug)]
struct Built {
    instrument: InstrumentAny,
    book: MergedBook,
}

nautilus_actor!(Unifier);

impl Unifier {
    #[must_use]
    pub fn new(unified: Vec<Unified>) -> Self {
        let unified_of = unified
            .iter()
            .flat_map(|u| u.members.iter().map(|m| (m.instrument_id, u.instrument_id)))
            .collect();
        Self {
            core: DataActorCore::new(DataActorConfig {
                actor_id: Some(ActorId::from("Unifier")),
                ..Default::default()
            }),
            unified,
            unified_of,
            venue_instruments: HashMap::new(),
            built: HashMap::new(),
            subscribed: HashSet::new(),
        }
    }

    fn unified(&self, instrument_id: InstrumentId) -> &Unified {
        self.unified
            .iter()
            .find(|u| u.instrument_id == instrument_id)
            .expect("unified_of only maps to known unified instruments")
    }

    fn build(&mut self, instrument_id: InstrumentId) -> anyhow::Result<()> {
        let unified = self.unified(instrument_id);
        let (loaded, missing): (Vec<Subscription>, Vec<Subscription>) = unified
            .members
            .iter()
            .partition(|m| self.venue_instruments.contains_key(&m.instrument_id));
        let members: Vec<_> = loaded
            .iter()
            .map(|m| self.venue_instruments[&m.instrument_id].clone())
            .collect();
        let instrument = build_instrument(instrument_id, unified.market, &members)?;

        log::info!(
            "built {instrument_id} (tick {}, size step {}) from {} venues",
            instrument.price_increment(),
            instrument.size_increment(),
            members.len()
        );
        for m in &missing {
            log::warn!(
                "{instrument_id} built without {}: instrument not loaded",
                m.instrument_id
            );
        }

        self.cache_rc()
            .borrow_mut()
            .add_instrument(instrument.clone())?;
        msgbus::publish_instrument(
            switchboard::get_instrument_topic(instrument_id),
            &instrument,
        );
        let book = MergedBook::new(
            instrument_id,
            instrument.price_increment(),
            instrument.size_precision(),
        );
        self.built.insert(instrument_id, Built { instrument, book });
        for m in loaded {
            self.subscribe_member(m);
        }
        Ok(())
    }

    fn subscribe_member(&mut self, sub: Subscription) {
        if !self.subscribed.insert(sub) {
            return;
        }
        log::info!("unifying {} from {}", sub.instrument_id, sub.client_id);
        self.subscribe_trades(sub.instrument_id, Some(sub.client_id), None);
        self.subscribe_book_deltas(
            sub.instrument_id,
            BookType::L2_MBP,
            sub.book_depth,
            Some(sub.client_id),
            false,
            None,
        );
    }
}

impl DataActor for Unifier {
    fn on_start(&mut self) -> anyhow::Result<()> {
        // Venue data is subscribed once the client has the instrument: some
        // load only part of their venue's instruments on connect (Coinbase:
        // spot only) and drop data for the rest, including the recent trades
        // Coinbase sends on subscribing.
        for sub in self
            .unified
            .iter()
            .flat_map(|u| u.members.clone())
            .collect::<Vec<_>>()
        {
            self.request_instrument(sub.instrument_id, None, None, Some(sub.client_id), None)?;
        }
        let deadline = self.clock().timestamp_ns() + BUILD_TIMEOUT;
        self.clock()
            .set_time_alert_ns(BUILD_TIMER, deadline, None, None)?;
        Ok(())
    }

    fn on_instrument(&mut self, instrument: &InstrumentAny) -> anyhow::Result<()> {
        let venue_id = instrument.id();
        let Some(&unified_id) = self.unified_of.get(&venue_id) else {
            return Ok(());
        };
        self.venue_instruments.insert(venue_id, instrument.clone());

        if self.built.contains_key(&unified_id) {
            log::warn!("{venue_id} loaded late, joins {unified_id} with its tick");
            let sub = *self
                .unified(unified_id)
                .members
                .iter()
                .find(|m| m.instrument_id == venue_id)
                .expect("unified_of maps members only");
            self.subscribe_member(sub);
        } else if self
            .unified(unified_id)
            .members
            .iter()
            .all(|m| self.venue_instruments.contains_key(&m.instrument_id))
        {
            self.build(unified_id)?;
        }
        Ok(())
    }

    fn on_time_event(&mut self, event: &TimeEvent) -> anyhow::Result<()> {
        if event.name != BUILD_TIMER {
            return Ok(());
        }
        let pending: Vec<_> = self
            .unified
            .iter()
            .map(|u| u.instrument_id)
            .filter(|id| !self.built.contains_key(id))
            .collect();
        for instrument_id in pending {
            if let Err(e) = self.build(instrument_id) {
                log::error!("cannot build {instrument_id}: {e}");
            }
        }
        Ok(())
    }

    fn on_stop(&mut self) -> anyhow::Result<()> {
        for sub in std::mem::take(&mut self.subscribed) {
            self.unsubscribe_trades(sub.instrument_id, Some(sub.client_id), None);
            self.unsubscribe_book_deltas(sub.instrument_id, Some(sub.client_id), None);
        }
        Ok(())
    }

    fn on_trade(&mut self, trade: &TradeTick) -> anyhow::Result<()> {
        let Some(unified_id) = self.unified_of.get(&trade.instrument_id) else {
            return Ok(());
        };
        let (Some(built), Some(venue_instrument)) = (
            self.built.get(unified_id),
            self.venue_instruments.get(&trade.instrument_id),
        ) else {
            return Ok(());
        };
        let trade = unify_trade(trade, venue_instrument, &built.instrument);
        log::debug!(
            "trade {} {:?} {} @ {} id={}",
            trade.instrument_id,
            trade.aggressor_side,
            trade.size,
            trade.price,
            trade.trade_id
        );
        self.cache_rc().borrow_mut().add_trade(trade)?;
        msgbus::publish_trade(switchboard::get_trades_topic(trade.instrument_id), &trade);
        Ok(())
    }

    fn on_book_deltas(&mut self, deltas: &OrderBookDeltas) -> anyhow::Result<()> {
        let ts_init = self.clock().timestamp_ns();
        let Some(unified_id) = self.unified_of.get(&deltas.instrument_id) else {
            return Ok(());
        };
        let (Some(built), Some(venue_instrument)) = (
            self.built.get_mut(unified_id),
            self.venue_instruments.get(&deltas.instrument_id),
        ) else {
            return Ok(());
        };
        if let Some(merged) = built
            .book
            .apply(deltas, venue_instrument.multiplier(), ts_init)
        {
            if log::log_enabled!(log::Level::Debug) {
                for delta in &merged.deltas {
                    log::debug!(
                        "book {} {:?} {} {} {} from {}",
                        delta.instrument_id,
                        delta.action,
                        if delta.order.side == Some(OrderSide::Buy) {
                            "bid"
                        } else {
                            "ask"
                        },
                        delta.order.price,
                        delta.order.size,
                        deltas.instrument_id
                    );
                }
            }
            msgbus::publish_deltas(
                switchboard::get_book_deltas_topic(merged.instrument_id),
                &merged,
            );
        }
        Ok(())
    }
}
