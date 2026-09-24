use std::collections::{HashMap, HashSet};

use nautilus_common::{
    actor::{DataActor, DataActorConfig, DataActorCore, DataActorNative},
    msgbus::{self, switchboard},
    nautilus_actor,
    timer::TimeEvent,
};
use nautilus_core::DurationNanos;
use nautilus_model::{
    data::{OrderBookDeltas, QuoteTick, TradeTick},
    enums::{BookType, OrderSide},
    identifiers::{ActorId, InstrumentId},
    instruments::{Instrument, InstrumentAny},
    types::Currency,
};

use super::{
    MergedBook, RateSource, Unified, UsdRate, build_instrument, rate_sources, unify_trade,
};
use crate::venue::Subscription;

const BUILD_TIMER: &str = "unifier-build";
/// How long to wait for every venue instrument and USD rate before building
/// a unified instrument from the venues that are ready, so one failing venue
/// doesn't hold back the others.
const BUILD_TIMEOUT: DurationNanos = DurationNanos::from_secs(30);
/// Levels per side of the merged book.
const BOOK_DEPTH: usize = 100;

/// Republishes the trades and books of each unified instrument's venues as
/// the unified instrument's own, with prices in USD.
///
/// A venue is ready once its instrument is loaded and, if it quotes in a
/// stablecoin, that coin's USD rate is known. The unified instrument is built
/// once every venue is ready, as its tick is the finest of theirs; venue data
/// is subscribed from then on.
#[derive(Debug)]
pub struct Unifier {
    core: DataActorCore,
    unified: Vec<Unified>,
    unified_of: HashMap<InstrumentId, InstrumentId>,
    venue_instruments: HashMap<InstrumentId, InstrumentAny>,
    built: HashMap<InstrumentId, Built>,
    subscribed: HashSet<Subscription>,
    rate_sources: Vec<RateSource>,
    rates: HashMap<&'static str, UsdRate>,
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
            rate_sources: rate_sources(),
            rates: HashMap::new(),
        }
    }

    fn unified(&self, instrument_id: InstrumentId) -> &Unified {
        self.unified
            .iter()
            .find(|u| u.instrument_id == instrument_id)
            .expect("unified_of only maps to known unified instruments")
    }

    /// `None` until the rate is known. Prices are in the currency a
    /// contract settles in: Hyperliquid perps quote in USD but settle, and
    /// so price, in USDC. For spot it is the quote currency.
    fn rate(&self, instrument: &InstrumentAny) -> Option<UsdRate> {
        match price_currency(instrument).code.as_str() {
            "USD" => Some(UsdRate::ONE),
            code => self.rates.get(code).copied(),
        }
    }

    fn venue_rate(&self, venue_id: InstrumentId) -> Option<UsdRate> {
        self.rate(self.venue_instruments.get(&venue_id)?)
    }

    fn is_ready(&self, sub: &Subscription) -> bool {
        self.venue_rate(sub.instrument_id).is_some()
    }

    /// Builds the unified instrument once all its venues are ready, or
    /// subscribes a venue that becomes ready after it was built.
    fn progress(&mut self, unified_id: InstrumentId) -> anyhow::Result<()> {
        let members = self.unified(unified_id).members.clone();
        if self.built.contains_key(&unified_id) {
            for sub in members {
                if self.is_ready(&sub) && !self.subscribed.contains(&sub) {
                    log::warn!("{} ready late, joins {unified_id}", sub.instrument_id);
                    self.subscribe_member(unified_id, sub);
                }
            }
        } else if members.iter().all(|m| self.is_ready(m)) {
            self.build(unified_id)?;
        }
        Ok(())
    }

    fn build(&mut self, instrument_id: InstrumentId) -> anyhow::Result<()> {
        let unified = self.unified(instrument_id);
        let (ready, missing): (Vec<Subscription>, Vec<Subscription>) =
            unified.members.iter().partition(|m| self.is_ready(m));
        let members: Vec<_> = ready
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
            let reason = match self.venue_instruments.get(&m.instrument_id) {
                Some(i) => format!("no USD rate for {}", price_currency(i)),
                None => "instrument not loaded".to_string(),
            };
            log::warn!(
                "{instrument_id} built without {}: {reason}",
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
            BOOK_DEPTH,
        );
        self.built.insert(instrument_id, Built { instrument, book });
        for m in ready {
            self.subscribe_member(instrument_id, m);
        }
        Ok(())
    }

    fn subscribe_member(&mut self, unified_id: InstrumentId, sub: Subscription) {
        if !self.subscribed.insert(sub) {
            return;
        }
        let rate = self
            .venue_rate(sub.instrument_id)
            .expect("only ready venues are subscribed");
        let ts_init = self.clock().timestamp_ns();
        if let Some(built) = self.built.get_mut(&unified_id) {
            // The venue has no levels yet: nothing to publish.
            let _ = built.book.set_rate(sub.instrument_id, rate, ts_init);
        }
        log::info!(
            "unifying {} from {} (USD rate {})",
            sub.instrument_id,
            sub.client_id,
            rate.as_f64()
        );
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

fn price_currency(instrument: &InstrumentAny) -> Currency {
    instrument.settlement_currency()
}

fn publish_book(merged: &OrderBookDeltas, cause: &str) {
    if log::log_enabled!(log::Level::Debug) {
        for delta in &merged.deltas {
            log::debug!(
                "book {} {:?} {} {} {} from {cause}",
                delta.instrument_id,
                delta.action,
                if delta.order.side == Some(OrderSide::Buy) {
                    "bid"
                } else {
                    "ask"
                },
                delta.order.price,
                delta.order.size,
            );
        }
    }
    msgbus::publish_deltas(
        switchboard::get_book_deltas_topic(merged.instrument_id),
        merged,
    );
}

impl DataActor for Unifier {
    fn on_start(&mut self) -> anyhow::Result<()> {
        // Data is subscribed once the client has the instrument: some load
        // only part of their venue's instruments on connect (Coinbase: spot
        // only) and drop data for the rest, including the recent trades
        // Coinbase sends on subscribing.
        let rate_ids: Vec<_> = self
            .rate_sources
            .iter()
            .map(|s| (s.instrument_id, s.client_id))
            .collect();
        let member_ids: Vec<_> = self
            .unified
            .iter()
            .flat_map(|u| u.members.iter().map(|m| (m.instrument_id, m.client_id)))
            .collect();
        for (instrument_id, client_id) in rate_ids.into_iter().chain(member_ids) {
            self.request_instrument(instrument_id, None, None, Some(client_id), None)?;
        }
        let deadline = self.clock().timestamp_ns() + BUILD_TIMEOUT;
        self.clock()
            .set_time_alert_ns(BUILD_TIMER, deadline, None, None)?;
        Ok(())
    }

    fn on_instrument(&mut self, instrument: &InstrumentAny) -> anyhow::Result<()> {
        let instrument_id = instrument.id();
        if let Some(source) = self
            .rate_sources
            .iter()
            .find(|s| s.instrument_id == instrument_id)
            .copied()
        {
            self.subscribe_quotes(source.instrument_id, Some(source.client_id), None);
            return Ok(());
        }
        let Some(&unified_id) = self.unified_of.get(&instrument_id) else {
            return Ok(());
        };
        self.venue_instruments
            .insert(instrument_id, instrument.clone());
        self.progress(unified_id)
    }

    fn on_quote(&mut self, quote: &QuoteTick) -> anyhow::Result<()> {
        let Some(source) = self
            .rate_sources
            .iter()
            .find(|s| s.instrument_id == quote.instrument_id)
            .copied()
        else {
            return Ok(());
        };
        let rate = UsdRate::from_quote(quote);
        if self.rates.insert(source.currency, rate) == Some(rate) {
            return Ok(());
        }
        log::debug!("USD rate of {} is {}", source.currency, rate.as_f64());

        let ts_init = self.clock().timestamp_ns();
        let quoted_in: Vec<_> = self
            .subscribed
            .iter()
            .filter(|sub| {
                price_currency(&self.venue_instruments[&sub.instrument_id]).code == source.currency
            })
            .map(|sub| sub.instrument_id)
            .collect();
        for venue_id in quoted_in {
            let unified_id = self.unified_of[&venue_id];
            if let Some(built) = self.built.get_mut(&unified_id)
                && let Some(merged) = built.book.set_rate(venue_id, rate, ts_init)
            {
                publish_book(&merged, source.currency);
            }
        }
        for unified_id in self
            .unified
            .iter()
            .map(|u| u.instrument_id)
            .collect::<Vec<_>>()
        {
            self.progress(unified_id)?;
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
        for source in self.rate_sources.clone() {
            self.unsubscribe_quotes(source.instrument_id, Some(source.client_id), None);
        }
        Ok(())
    }

    fn on_trade(&mut self, trade: &TradeTick) -> anyhow::Result<()> {
        let Some(unified_id) = self.unified_of.get(&trade.instrument_id) else {
            return Ok(());
        };
        let (Some(built), Some(venue_instrument), Some(rate)) = (
            self.built.get(unified_id),
            self.venue_instruments.get(&trade.instrument_id),
            self.venue_rate(trade.instrument_id),
        ) else {
            return Ok(());
        };
        let trade = unify_trade(trade, venue_instrument, &built.instrument, rate);
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
            publish_book(&merged, &deltas.instrument_id.to_string());
        }
        Ok(())
    }
}
