use std::collections::{BTreeMap, HashMap};

use nautilus_common::{
    actor::{DataActor, DataActorConfig, DataActorCore},
    nautilus_actor,
    timer::TimeEvent,
};
use nautilus_core::{DurationNanos, UnixNanos};
use nautilus_model::{
    data::TradeTick,
    identifiers::{ActorId, InstrumentId},
    instruments::{Instrument, InstrumentAny},
    types::{Quantity, quantity::QuantityRaw},
};

use crate::{
    unified::{self, Unified, size_in_coins},
    venue::Subscription,
};

const MINUTE: DurationNanos = DurationNanos::from_secs(60);
const TIMER: &str = "volume-log";

/// Logs, per minute, the volume of each unified instrument next to the
/// volumes of its venues, counted from the venue trades directly: the two
/// must match.
///
/// Venue trades are subscribed when the unified instrument is published,
/// right as the [`Unifier`](crate::unified::Unifier) subscribes to them.
#[derive(Debug)]
pub struct VolumeLogger {
    core: DataActorCore,
    unified: Vec<Unified>,
    /// Minute start, by when trades reached the node (`ts_init`), to volume
    /// in coins per instrument. A minute is complete as soon as it is over.
    minutes: BTreeMap<UnixNanos, HashMap<InstrumentId, QuantityRaw>>,
    /// The first minute after subscribing, per unified instrument; earlier
    /// ones are not logged. The minute of the subscription has only its
    /// tail, and minutes before it only the recent trades some venues
    /// replay on subscribing (Coinbase: its last 100).
    first_minute: HashMap<InstrumentId, UnixNanos>,
}

nautilus_actor!(VolumeLogger);

impl VolumeLogger {
    #[must_use]
    pub fn new(unified: Vec<Unified>) -> Self {
        Self {
            core: DataActorCore::new(DataActorConfig {
                actor_id: Some(ActorId::from("VolumeLogger")),
                ..Default::default()
            }),
            unified,
            minutes: BTreeMap::new(),
            first_minute: HashMap::new(),
        }
    }

    fn unified_ids(&self) -> Vec<InstrumentId> {
        self.unified.iter().map(|u| u.instrument_id).collect()
    }

    fn members(&self) -> Vec<Subscription> {
        self.unified
            .iter()
            .flat_map(|u| u.members.clone())
            .collect()
    }

    fn log_minute(&self, minute: UnixNanos, volumes: &HashMap<InstrumentId, QuantityRaw>) {
        let time = minute.to_datetime_utc();
        let volume = |id| volumes.get(&id).copied().unwrap_or(0);
        for unified in &self.unified {
            if self
                .first_minute
                .get(&unified.instrument_id)
                .is_none_or(|&first| minute < first)
            {
                continue;
            }
            let total = volume(unified.instrument_id);
            let venues: Vec<_> = unified
                .members
                .iter()
                .map(|m| (m.venue, volume(m.instrument_id)))
                .collect();
            let sum: QuantityRaw = venues.iter().map(|&(_, v)| v).sum();
            let venues: Vec<_> = venues
                .iter()
                .map(|(venue, v)| format!("{venue} {}", coins(*v)))
                .collect();
            let line = format!(
                "{} {} volume {} = {} ({})",
                unified.instrument_id,
                time.strftime("%H:%M"),
                coins(total),
                coins(sum),
                venues.join(", ")
            );
            if total == sum {
                log::info!("{line}");
            } else {
                log::warn!("{line}: unified volume differs from the sum of its venues");
            }
        }
    }
}

fn coins(raw: QuantityRaw) -> Quantity {
    Quantity::from_raw(raw, 4)
}

impl DataActor for VolumeLogger {
    fn on_start(&mut self) -> anyhow::Result<()> {
        for id in self.unified_ids() {
            self.subscribe_instrument(id, None, None);
            self.subscribe_trades(id, None, None);
        }
        self.clock()
            .set_timer_ns(TIMER, MINUTE, None, None, None, None, None)?;
        Ok(())
    }

    fn on_instrument(&mut self, instrument: &InstrumentAny) -> anyhow::Result<()> {
        let Some(unified) = self
            .unified
            .iter()
            .find(|u| u.instrument_id == instrument.id())
        else {
            return Ok(());
        };
        // Route by client, not by venue: one venue can have several clients
        // (Binance spot and futures), none of them named after the venue.
        // A venue instrument not loaded by now is left out of the unified
        // instrument, and its client would drop trades for it.
        let members: Vec<_> = unified
            .members
            .iter()
            .filter(|m| self.cache().instrument(&m.instrument_id).is_some())
            .copied()
            .collect();
        let first = self.clock().timestamp_ns().floor(MINUTE) + MINUTE;
        self.first_minute.insert(instrument.id(), first);
        for sub in members {
            self.subscribe_trades(sub.instrument_id, Some(sub.client_id), None);
        }
        Ok(())
    }

    fn on_stop(&mut self) -> anyhow::Result<()> {
        for sub in self.members() {
            self.unsubscribe_trades(sub.instrument_id, Some(sub.client_id), None);
        }
        for id in self.unified_ids() {
            self.unsubscribe_trades(id, None, None);
            self.unsubscribe_instrument(id, None, None);
        }
        Ok(())
    }

    fn on_trade(&mut self, trade: &TradeTick) -> anyhow::Result<()> {
        let size = if trade.instrument_id.venue.as_str() == unified::VENUE {
            trade.size
        } else {
            let Some(instrument) = self.cache().instrument(&trade.instrument_id) else {
                log::warn!("no instrument for {}, trade dropped", trade.instrument_id);
                return Ok(());
            };
            size_in_coins(trade.size, &instrument)
        };
        *self
            .minutes
            .entry(trade.ts_init.floor(MINUTE))
            .or_default()
            .entry(trade.instrument_id)
            .or_default() += size.raw();
        Ok(())
    }

    fn on_time_event(&mut self, event: &TimeEvent) -> anyhow::Result<()> {
        if event.name != TIMER {
            return Ok(());
        }
        let open = event.ts_event.floor(MINUTE);
        let still_open = self.minutes.split_off(&open);
        let closed = std::mem::replace(&mut self.minutes, still_open);
        for (minute, volumes) in &closed {
            self.log_minute(*minute, volumes);
        }
        Ok(())
    }
}
