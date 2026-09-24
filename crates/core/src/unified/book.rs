use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::UsdRate;
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{BookOrder, OrderBookDelta, OrderBookDeltas},
    enums::{BookAction, OrderSide, RecordFlag},
    identifiers::InstrumentId,
    types::{Price, Quantity, price::PriceRaw, quantity::QuantityRaw},
};

/// The L2 books of several venues summed per price bucket of the unified tick,
/// kept up to date one venue batch at a time.
///
/// The published book holds the best `depth` buckets per side, each summing
/// whichever venues quote there: a shallow venue (Hyperliquid sends 20
/// levels) only adds to the buckets it reaches. It can be crossed, since
/// venues' books overlap.
#[derive(Debug)]
pub struct MergedBook {
    instrument_id: InstrumentId,
    tick: PriceRaw,
    price_precision: u8,
    size_precision: u8,
    bids: Ladder,
    asks: Ladder,
    /// Venues in the middle of a snapshot split over several batches.
    open_snapshots: BTreeSet<InstrumentId>,
    /// Venues quoting in something other than USD.
    rates: HashMap<InstrumentId, UsdRate>,
    sequence: u64,
    /// Venues' clocks and latencies differ, so their batches interleave out
    /// of `ts_event` order; the merged book never goes back in time, or a
    /// Nautilus `OrderBook` fed with it warns on every such batch.
    ts_event: UnixNanos,
}

#[derive(Debug)]
struct Ladder {
    side: OrderSide,
    /// Venue price level to size in coins.
    venues: HashMap<InstrumentId, BTreeMap<PriceRaw, QuantityRaw>>,
    /// Bucket price to the sum over venues.
    totals: BTreeMap<PriceRaw, QuantityRaw>,
    published: BTreeMap<PriceRaw, QuantityRaw>,
    depth: usize,
    /// The deepest published bucket.
    bound: Option<PriceRaw>,
}

impl MergedBook {
    pub fn new(instrument_id: InstrumentId, tick: Price, size_precision: u8, depth: usize) -> Self {
        Self {
            instrument_id,
            tick: tick.raw(),
            price_precision: tick.precision,
            size_precision,
            bids: Ladder::new(OrderSide::Buy, depth),
            asks: Ladder::new(OrderSide::Sell, depth),
            open_snapshots: BTreeSet::new(),
            rates: HashMap::new(),
            sequence: 0,
            ts_event: UnixNanos::default(),
        }
    }

    /// Applies one venue batch, with sizes multiplied by `multiplier` into
    /// coins, and returns the deltas that bring the published book up to
    /// date, if any.
    ///
    /// A snapshot (or a clear) first drops everything the venue contributed,
    /// so a venue that resyncs never leaves stale levels behind.
    pub fn apply(
        &mut self,
        deltas: &OrderBookDeltas,
        multiplier: Quantity,
        ts_init: UnixNanos,
    ) -> Option<OrderBookDeltas> {
        let venue = deltas.instrument_id;
        let rate = self.rate(venue);
        let mut touched = (BTreeSet::new(), BTreeSet::new());

        for delta in &deltas.deltas {
            let is_snapshot = RecordFlag::F_SNAPSHOT.matches(delta.flags);
            if delta.action == BookAction::Clear
                || (is_snapshot && !self.open_snapshots.contains(&venue))
            {
                self.bids
                    .remove_venue(venue, self.tick, rate, &mut touched.0);
                self.asks
                    .remove_venue(venue, self.tick, rate, &mut touched.1);
            }
            if is_snapshot && !RecordFlag::F_LAST.matches(delta.flags) {
                self.open_snapshots.insert(venue);
            } else {
                self.open_snapshots.remove(&venue);
            }

            let size = match delta.action {
                BookAction::Clear => continue,
                BookAction::Delete => 0,
                BookAction::Add | BookAction::Update => (delta.order.size * multiplier).raw(),
            };
            let price = delta.order.price.raw();
            match delta.order.side {
                Some(OrderSide::Buy) => {
                    self.bids
                        .set(venue, price, size, self.tick, rate, &mut touched.0);
                }
                Some(OrderSide::Sell) => {
                    self.asks
                        .set(venue, price, size, self.tick, rate, &mut touched.1);
                }
                None => {}
            }
        }

        if let Some(last) = deltas.deltas.last() {
            self.ts_event = self.ts_event.max(last.ts_event);
        }
        self.publish(touched, ts_init)
    }

    fn rate(&self, venue: InstrumentId) -> UsdRate {
        self.rates.get(&venue).copied().unwrap_or(UsdRate::ONE)
    }

    /// Sets the USD rate of a venue's quote currency, moving its levels to
    /// the buckets of their new USD prices.
    pub fn set_rate(
        &mut self,
        venue: InstrumentId,
        rate: UsdRate,
        ts_init: UnixNanos,
    ) -> Option<OrderBookDeltas> {
        let old = self.rates.insert(venue, rate).unwrap_or(UsdRate::ONE);
        if old == rate {
            return None;
        }
        let mut touched = (BTreeSet::new(), BTreeSet::new());
        self.bids
            .rebucket(venue, self.tick, old, rate, &mut touched.0);
        self.asks
            .rebucket(venue, self.tick, old, rate, &mut touched.1);
        self.publish(touched, ts_init)
    }

    fn publish(
        &mut self,
        touched: (BTreeSet<PriceRaw>, BTreeSet<PriceRaw>),
        ts_init: UnixNanos,
    ) -> Option<OrderBookDeltas> {
        let mut changes = self.bids.publish(touched.0);
        changes.extend(self.asks.publish(touched.1));
        self.emit(&changes, self.ts_event, ts_init)
    }

    fn emit(
        &mut self,
        changes: &[Change],
        ts_event: UnixNanos,
        ts_init: UnixNanos,
    ) -> Option<OrderBookDeltas> {
        if changes.is_empty() {
            return None;
        }
        self.sequence += 1;
        let last = changes.len() - 1;
        let deltas = changes
            .iter()
            .enumerate()
            .map(|(i, change)| {
                let order = BookOrder::new(
                    change.side,
                    Price::from_raw(change.price, self.price_precision),
                    Quantity::from_raw(change.size, self.size_precision),
                    0,
                );
                let flags = if i == last {
                    RecordFlag::F_LAST as u8
                } else {
                    0
                };
                OrderBookDelta::new(
                    self.instrument_id,
                    change.action,
                    order,
                    flags,
                    self.sequence,
                    ts_event,
                    ts_init,
                )
            })
            .collect();
        Some(OrderBookDeltas::new(self.instrument_id, deltas))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Change {
    side: OrderSide,
    action: BookAction,
    price: PriceRaw,
    size: QuantityRaw,
}

impl Ladder {
    fn new(side: OrderSide, depth: usize) -> Self {
        Self {
            side,
            depth,
            venues: HashMap::new(),
            totals: BTreeMap::new(),
            published: BTreeMap::new(),
            bound: None,
        }
    }

    /// Bids round down and asks up, so a venue off the unified grid never
    /// shows a better price than it quotes.
    fn bucket(&self, price: PriceRaw, tick: PriceRaw) -> PriceRaw {
        let floor = price.div_euclid(tick) * tick;
        match self.side {
            OrderSide::Sell if floor != price => floor + tick,
            OrderSide::Buy | OrderSide::Sell => floor,
        }
    }

    fn set(
        &mut self,
        venue: InstrumentId,
        price: PriceRaw,
        size: QuantityRaw,
        tick: PriceRaw,
        rate: UsdRate,
        touched: &mut BTreeSet<PriceRaw>,
    ) {
        let levels = self.venues.entry(venue).or_default();
        let old = if size == 0 {
            levels.remove(&price)
        } else {
            levels.insert(price, size)
        }
        .unwrap_or(0);
        if old != size {
            self.add_to_total(self.bucket(rate.convert(price), tick), old, size, touched);
        }
    }

    fn remove_venue(
        &mut self,
        venue: InstrumentId,
        tick: PriceRaw,
        rate: UsdRate,
        touched: &mut BTreeSet<PriceRaw>,
    ) {
        let Some(levels) = self.venues.remove(&venue) else {
            return;
        };
        for (price, size) in levels {
            self.add_to_total(self.bucket(rate.convert(price), tick), size, 0, touched);
        }
    }

    fn rebucket(
        &mut self,
        venue: InstrumentId,
        tick: PriceRaw,
        old: UsdRate,
        new: UsdRate,
        touched: &mut BTreeSet<PriceRaw>,
    ) {
        let Some(levels) = self.venues.get(&venue) else {
            return;
        };
        let moves: Vec<_> = levels
            .iter()
            .map(|(&price, &size)| {
                let from = self.bucket(old.convert(price), tick);
                let to = self.bucket(new.convert(price), tick);
                (from, to, size)
            })
            .collect();
        for (from, to, size) in moves {
            if from != to {
                self.add_to_total(from, size, 0, touched);
                self.add_to_total(to, 0, size, touched);
            }
        }
    }

    fn add_to_total(
        &mut self,
        bucket: PriceRaw,
        old: QuantityRaw,
        new: QuantityRaw,
        touched: &mut BTreeSet<PriceRaw>,
    ) {
        let total = self.totals.entry(bucket).or_insert(0);
        *total = *total + new - old;
        if *total == 0 {
            self.totals.remove(&bucket);
        }
        touched.insert(bucket);
    }

    fn depth_bound(&self) -> Option<PriceRaw> {
        let deepest = self.depth.min(self.totals.len()).checked_sub(1)?;
        match self.side {
            OrderSide::Buy => self.totals.keys().rev().nth(deepest),
            OrderSide::Sell => self.totals.keys().nth(deepest),
        }
        .copied()
    }

    fn is_visible(&self, bucket: PriceRaw) -> bool {
        self.bound.is_some_and(|bound| match self.side {
            OrderSide::Buy => bucket >= bound,
            OrderSide::Sell => bucket <= bound,
        })
    }

    fn publish(&mut self, mut touched: BTreeSet<PriceRaw>) -> Vec<Change> {
        let bound = self.depth_bound();
        if bound != self.bound {
            // Buckets between the old and the new bound enter or leave the range.
            let range = match (self.bound, bound) {
                (Some(a), Some(b)) => a.min(b)..=a.max(b),
                _ => PriceRaw::MIN..=PriceRaw::MAX,
            };
            touched.extend(self.totals.range(range.clone()).map(|(&price, _)| price));
            touched.extend(self.published.range(range).map(|(&price, _)| price));
            self.bound = bound;
        }

        let mut changes = Vec::new();
        for price in touched {
            let desired = self
                .is_visible(price)
                .then(|| self.totals.get(&price).copied())
                .flatten();
            let action = match (self.published.get(&price).copied(), desired) {
                (None, Some(_)) => BookAction::Add,
                (Some(old), Some(new)) if old != new => BookAction::Update,
                (Some(_), None) => BookAction::Delete,
                _ => continue,
            };
            match desired {
                Some(size) => self.published.insert(price, size),
                None => self.published.remove(&price),
            };
            changes.push(Change {
                side: self.side,
                action,
                price,
                size: desired.unwrap_or(0),
            });
        }
        changes
    }
}

#[cfg(test)]
mod tests {
    use nautilus_model::orderbook::OrderBook;

    use super::*;

    const UNIFIED: &str = "BTC-PERP.SVP";
    const A: &str = "A-PERP.X";
    const B: &str = "B-PERP.Y";

    fn delta(
        venue: &str,
        action: BookAction,
        side: OrderSide,
        price: &str,
        quantity: &str,
    ) -> OrderBookDelta {
        let order = BookOrder::new(side, Price::from(price), Quantity::from(quantity), 0);
        OrderBookDelta::new(
            InstrumentId::from(venue),
            action,
            order,
            0,
            0,
            0.into(),
            0.into(),
        )
    }

    fn bid(venue: &str, price: &str, size: &str) -> OrderBookDelta {
        delta(venue, BookAction::Update, OrderSide::Buy, price, size)
    }

    fn ask(venue: &str, price: &str, size: &str) -> OrderBookDelta {
        delta(venue, BookAction::Update, OrderSide::Sell, price, size)
    }

    fn snapshot(mut deltas: Vec<OrderBookDelta>) -> Vec<OrderBookDelta> {
        let last = deltas.len() - 1;
        for (i, d) in deltas.iter_mut().enumerate() {
            d.flags = RecordFlag::F_SNAPSHOT as u8;
            if i == last {
                d.flags |= RecordFlag::F_LAST as u8;
            }
        }
        deltas
    }

    /// Applies batches to a `MergedBook` and its output to a Nautilus book.
    struct Harness {
        merged: MergedBook,
        book: OrderBook,
    }

    impl Harness {
        fn new() -> Self {
            Self::with_depth(100)
        }

        fn with_depth(depth: usize) -> Self {
            let id = InstrumentId::from(UNIFIED);
            Self {
                merged: MergedBook::new(id, Price::from("0.1"), 4, depth),
                book: OrderBook::new(id, nautilus_model::enums::BookType::L2_MBP),
            }
        }

        fn apply(&mut self, deltas: Vec<OrderBookDelta>) -> Option<OrderBookDeltas> {
            self.apply_with(deltas, "1")
        }

        fn apply_with(
            &mut self,
            deltas: Vec<OrderBookDelta>,
            multiplier: &str,
        ) -> Option<OrderBookDeltas> {
            let venue = deltas[0].instrument_id;
            let out = self.merged.apply(
                &OrderBookDeltas::new(venue, deltas),
                Quantity::from(multiplier),
                0.into(),
            );
            if let Some(out) = &out {
                self.book.apply_deltas(out).unwrap();
            }
            out
        }

        fn bids(&self) -> Vec<(String, String)> {
            self.book.bids(None).map(printed).collect()
        }

        fn asks(&self) -> Vec<(String, String)> {
            self.book.asks(None).map(printed).collect()
        }

        fn raw_levels(&self, side: OrderSide) -> BTreeMap<PriceRaw, QuantityRaw> {
            let levels: Vec<_> = match side {
                OrderSide::Buy => self.book.bids(None).collect(),
                OrderSide::Sell => self.book.asks(None).collect(),
            };
            levels
                .into_iter()
                .map(|l| (l.price.value.raw(), l.size_raw()))
                .collect()
        }
    }

    fn printed(level: &nautilus_model::orderbook::BookLevel) -> (String, String) {
        (
            level.price.value.to_string(),
            level.size_decimal().to_string(),
        )
    }

    fn levels(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|&(p, s)| (p.into(), s.into())).collect()
    }

    #[test]
    fn sums_venues_per_price() {
        let mut h = Harness::new();
        h.apply(vec![bid(A, "100.0", "1"), ask(A, "100.2", "2")]);
        h.apply(vec![bid(B, "100.0", "0.5"), ask(B, "100.2", "0.25")]);
        assert_eq!(h.bids(), levels(&[("100.0", "1.5000")]));
        assert_eq!(h.asks(), levels(&[("100.2", "2.2500")]));
    }

    #[test]
    fn converts_contracts_to_coins() {
        let mut h = Harness::new();
        h.apply_with(vec![bid(A, "100.0", "3")], "0.01");
        assert_eq!(h.bids(), levels(&[("100.0", "0.0300")]));
    }

    #[test]
    fn buckets_off_grid_prices_away_from_the_spread() {
        let mut h = Harness::new();
        h.apply(vec![bid(A, "100.05", "1"), ask(A, "100.25", "1")]);
        assert_eq!(h.bids(), levels(&[("100.0", "1.0000")]));
        assert_eq!(h.asks(), levels(&[("100.3", "1.0000")]));
    }

    #[test]
    fn deletes_a_level_when_its_last_venue_leaves() {
        let mut h = Harness::new();
        h.apply(vec![bid(A, "100.0", "1"), bid(A, "99.9", "1")]);
        h.apply(vec![bid(B, "100.0", "1"), bid(B, "99.9", "1")]);
        h.apply(vec![delta(
            A,
            BookAction::Delete,
            OrderSide::Buy,
            "100.0",
            "0",
        )]);
        assert_eq!(h.bids(), levels(&[("100.0", "1.0000"), ("99.9", "2.0000")]));
        let out = h
            .apply(vec![delta(
                B,
                BookAction::Delete,
                OrderSide::Buy,
                "100.0",
                "0",
            )])
            .unwrap();
        assert_eq!(out.deltas.len(), 1);
        assert_eq!(out.deltas[0].action, BookAction::Delete);
        assert_eq!(h.bids(), levels(&[("99.9", "2.0000")]));
    }

    #[test]
    fn publishes_the_best_levels_up_to_its_depth() {
        let mut h = Harness::with_depth(2);
        h.apply(vec![bid(A, "99.0", "1"), bid(A, "98.0", "1")]);
        assert_eq!(h.bids(), levels(&[("99.0", "1.0000"), ("98.0", "1.0000")]));

        // A better bid pushes 98.0 out.
        h.apply(vec![bid(B, "100.0", "1")]);
        assert_eq!(h.bids(), levels(&[("100.0", "1.0000"), ("99.0", "1.0000")]));

        // It leaves again: 98.0 comes back.
        h.apply(vec![delta(
            B,
            BookAction::Delete,
            OrderSide::Buy,
            "100.0",
            "0",
        )]);
        assert_eq!(h.bids(), levels(&[("99.0", "1.0000"), ("98.0", "1.0000")]));
    }

    #[test]
    fn a_shallow_venue_adds_only_where_it_quotes() {
        let mut h = Harness::new();
        h.apply(vec![
            bid(A, "100.0", "1"),
            bid(A, "99.0", "1"),
            bid(A, "98.0", "1"),
        ]);
        h.apply(vec![bid(B, "100.0", "1")]);
        assert_eq!(
            h.bids(),
            levels(&[("100.0", "2.0000"), ("99.0", "1.0000"), ("98.0", "1.0000")])
        );
    }

    #[test]
    fn a_snapshot_replaces_the_venue_contribution() {
        let mut h = Harness::new();
        h.apply(vec![bid(A, "100.0", "1"), bid(A, "99.0", "1")]);
        h.apply(vec![bid(B, "100.0", "1"), bid(B, "99.0", "1")]);
        h.apply(snapshot(vec![bid(A, "99.5", "3"), bid(A, "99.0", "3")]));
        assert_eq!(
            h.bids(),
            levels(&[("100.0", "1.0000"), ("99.5", "3.0000"), ("99.0", "4.0000")])
        );
    }

    #[test]
    fn a_snapshot_split_over_batches_is_kept_whole() {
        let mut h = Harness::new();
        let mut parts = snapshot(vec![bid(A, "100.0", "1"), bid(A, "99.0", "1")]);
        let second = parts.pop().unwrap();
        h.apply(parts);
        h.apply(vec![second]);
        assert_eq!(h.bids().len(), 2);
    }

    #[test]
    fn a_cleared_venue_drops_out_until_it_resyncs() {
        let mut h = Harness::new();
        h.apply(vec![bid(A, "100.0", "1"), bid(A, "98.0", "1")]);
        h.apply(vec![bid(B, "100.0", "1"), bid(B, "99.0", "1")]);
        assert_eq!(h.bids().len(), 3);

        let clear = OrderBookDelta::clear(InstrumentId::from(B), 0, 0.into(), 0.into());
        h.apply(vec![clear]);
        assert_eq!(h.bids(), levels(&[("100.0", "1.0000"), ("98.0", "1.0000")]));
    }

    #[test]
    fn crossed_books_are_kept() {
        let mut h = Harness::new();
        h.apply(vec![bid(A, "100.0", "1"), bid(A, "99.0", "1")]);
        h.apply(vec![ask(A, "100.1", "1"), ask(A, "101.0", "1")]);
        h.apply(vec![bid(B, "100.2", "1"), bid(B, "98.0", "1")]);
        h.apply(vec![ask(B, "100.3", "1"), ask(B, "102.0", "1")]);
        assert_eq!(h.book.best_bid_price(), Some(Price::from("100.2")));
        assert_eq!(h.book.best_ask_price(), Some(Price::from("100.1")));
        assert_eq!(h.bids().len(), 4);
        assert_eq!(h.asks().len(), 4);
    }

    #[test]
    fn a_batch_without_visible_change_emits_nothing() {
        let mut h = Harness::new();
        h.apply(vec![bid(A, "100.0", "1")]);
        assert!(h.apply(vec![bid(A, "100.0", "1")]).is_none());
    }

    fn usdt_at(bid: &str, ask: &str) -> UsdRate {
        UsdRate::from_quote(&nautilus_model::data::QuoteTick::new(
            InstrumentId::from("USDT/USD.KRAKEN"),
            Price::from(bid),
            Price::from(ask),
            Quantity::from(1),
            Quantity::from(1),
            0.into(),
            0.into(),
        ))
    }

    #[test]
    fn a_venue_rate_moves_its_levels_to_usd() {
        let mut h = Harness::new();
        let venue = InstrumentId::from(A);
        h.merged
            .set_rate(venue, usdt_at("0.998", "0.998"), 0.into());
        h.apply(vec![bid(A, "1000.0", "1"), ask(A, "1001.0", "1")]);
        h.apply(vec![bid(B, "998.0", "2")]);
        assert_eq!(h.bids(), levels(&[("998.0", "3.0000")]));
        assert_eq!(h.asks(), levels(&[("999.0", "1.0000")]));

        let out = h
            .merged
            .set_rate(venue, usdt_at("0.999", "0.999"), 0.into())
            .unwrap();
        h.book.apply_deltas(&out).unwrap();
        assert_eq!(
            h.bids(),
            levels(&[("999.0", "1.0000"), ("998.0", "2.0000")])
        );
        assert_eq!(h.asks(), levels(&[("1000.0", "1.0000")]));
    }

    #[test]
    fn never_goes_back_in_time() {
        let mut h = Harness::new();
        let mut late = bid(A, "100.0", "1");
        late.ts_event = 20.into();
        let mut early = bid(B, "100.0", "1");
        early.ts_event = 10.into();
        h.apply(vec![late]);
        let out = h.apply(vec![early]).unwrap();
        assert_eq!(out.ts_event, UnixNanos::from(20));
    }

    #[test]
    fn marks_the_last_delta_of_each_batch() {
        let mut h = Harness::new();
        let out = h
            .apply(vec![bid(A, "100.0", "1"), ask(A, "100.1", "1")])
            .unwrap();
        let flags: Vec<_> = out.deltas.iter().map(|d| d.flags).collect();
        assert_eq!(flags, [0, RecordFlag::F_LAST as u8]);
    }

    type Levels = BTreeMap<PriceRaw, QuantityRaw>;

    /// A venue's bids and asks, kept independently of `MergedBook`.
    type Model = HashMap<&'static str, [Levels; 2]>;

    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % n
        }

        fn level(&mut self) -> (OrderSide, String) {
            let side = if self.below(2) == 0 {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            };
            // Hundredths on a tick of tenths: half the prices are off grid.
            let cents = 9_900 + self.below(200);
            (side, format!("{}.{:02}", cents / 100, cents % 100))
        }
    }

    /// Sets a level in the model and returns the matching delta.
    fn set_level(
        model: &mut Model,
        venue: &'static str,
        action: BookAction,
        (side, price): (OrderSide, String),
        quantity: u64,
    ) -> OrderBookDelta {
        let levels = &mut model.entry(venue).or_default()[usize::from(side == OrderSide::Sell)];
        let raw = Price::from(price.as_str()).raw();
        let quantity = quantity.to_string();
        if action == BookAction::Delete {
            levels.remove(&raw);
        } else {
            levels.insert(raw, Quantity::from(quantity.as_str()).raw());
        }
        delta(venue, action, side, &price, &quantity)
    }

    /// The model's venues summed per bucket, the best `depth` of them.
    fn expected(model: &Model, side: OrderSide, depth: usize) -> Levels {
        let tick = Price::from("0.1").raw();
        let bucket = |p: PriceRaw| match side {
            OrderSide::Buy => p.div_euclid(tick) * tick,
            OrderSide::Sell => (p + tick - 1).div_euclid(tick) * tick,
        };
        let index = usize::from(side == OrderSide::Sell);
        let mut summed = Levels::new();
        for (&price, &quantity) in model.values().flat_map(|venue| &venue[index]) {
            *summed.entry(bucket(price)).or_default() += quantity;
        }
        let best: Vec<_> = match side {
            OrderSide::Buy => summed.into_iter().rev().take(depth).collect(),
            OrderSide::Sell => summed.into_iter().take(depth).collect(),
        };
        best.into_iter().collect()
    }

    /// Random updates, deletes, clears and (split) snapshots from three
    /// venues, some off the unified grid: after every batch, the published
    /// book must equal the best buckets of the venue books summed.
    #[test]
    fn matches_the_summed_venue_books_under_random_updates() {
        const VENUES: [&str; 3] = [A, B, "C-PERP.Z"];
        const DEPTH: usize = 10;
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        let mut h = Harness::with_depth(DEPTH);
        let mut model = Model::new();

        for _ in 0..5_000 {
            let venue = VENUES[usize::try_from(rng.below(3)).unwrap()];
            let batch = match rng.below(20) {
                0 => {
                    model.remove(venue);
                    let id = InstrumentId::from(venue);
                    vec![OrderBookDelta::clear(id, 0, 0.into(), 0.into())]
                }
                1 => {
                    model.remove(venue);
                    let deltas: Vec<_> = (0..=rng.below(15))
                        .map(|_| {
                            let level = rng.level();
                            let quantity = 1 + rng.below(5);
                            set_level(&mut model, venue, BookAction::Add, level, quantity)
                        })
                        .collect();
                    let mut deltas = snapshot(deltas);
                    if deltas.len() > 1 && rng.below(2) == 0 {
                        let tail = deltas.split_off(deltas.len() / 2);
                        h.apply(deltas);
                        tail
                    } else {
                        deltas
                    }
                }
                _ => {
                    let level = rng.level();
                    let action = match rng.below(4) {
                        0 => BookAction::Delete,
                        _ => BookAction::Update,
                    };
                    let quantity = 1 + rng.below(3);
                    vec![set_level(&mut model, venue, action, level, quantity)]
                }
            };
            h.apply(batch);

            for side in [OrderSide::Buy, OrderSide::Sell] {
                assert_eq!(
                    h.raw_levels(side),
                    expected(&model, side, DEPTH),
                    "{side:?}"
                );
            }
        }
    }
}
