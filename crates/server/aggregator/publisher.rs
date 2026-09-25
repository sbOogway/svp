//! Turns the unified instruments' trades and books into wire
//! [`Message`]s for the sinks.

use nautilus_common::{
    actor::{DataActor, DataActorConfig, DataActorCore},
    nautilus_actor,
};
use nautilus_model::{
    data::{OrderBookDeltas, TradeTick},
    enums::{AggressorSide, BookAction, BookType, OrderSide},
    identifiers::{ActorId, InstrumentId},
    types::{Price, Quantity, fixed::FIXED_PRECISION, price::PriceRaw, quantity::QuantityRaw},
};
use svp_common::protocol::{self, BookData, BookSide, BookUpdate, Message, Side, Trade};

use crate::aggregator::sink::Sink;

/// Subscribes to the unified instruments and feeds their trades and book
/// updates to the sinks.
#[derive(Debug)]
pub struct Publisher {
    core: DataActorCore,
    instrument_ids: Vec<InstrumentId>,
    sinks: Vec<Box<dyn Sink>>,
}

nautilus_actor!(Publisher);

impl Publisher {
    #[must_use]
    pub fn new(instrument_ids: Vec<InstrumentId>, sinks: Vec<Box<dyn Sink>>) -> Self {
        Self {
            core: DataActorCore::new(DataActorConfig {
                actor_id: Some(ActorId::from("Publisher")),
                ..Default::default()
            }),
            instrument_ids,
            sinks,
        }
    }

    fn send(&mut self, message: &Message) {
        for sink in &mut self.sinks {
            sink.send(message);
        }
    }
}

impl DataActor for Publisher {
    fn on_start(&mut self) -> anyhow::Result<()> {
        for id in self.instrument_ids.clone() {
            self.subscribe_trades(id, None, None);
            self.subscribe_book_deltas(id, BookType::L2_MBP, None, None, false, None);
        }
        Ok(())
    }

    fn on_stop(&mut self) -> anyhow::Result<()> {
        for id in self.instrument_ids.clone() {
            self.unsubscribe_trades(id, None, None);
            self.unsubscribe_book_deltas(id, None, None);
        }
        Ok(())
    }

    fn on_trade(&mut self, trade: &TradeTick) -> anyhow::Result<()> {
        if let Some(message) = trade_message(trade) {
            self.send(&message);
        }
        Ok(())
    }

    fn on_book_deltas(&mut self, deltas: &OrderBookDeltas) -> anyhow::Result<()> {
        for message in book_messages(deltas) {
            self.send(&message);
        }
        Ok(())
    }
}

const PRICE_UNIT: PriceRaw = PriceRaw::pow(
    10,
    FIXED_PRECISION as u32 - protocol::Price::ATOMIC_SCALE as u32,
);
const SIZE_UNIT: QuantityRaw = QuantityRaw::pow(
    10,
    FIXED_PRECISION as u32 - protocol::Quantity::QTY_SCALE as u32,
);

/// The nearest unit, half away from zero; `None` past an `i64`.
pub fn price_units(price: Price) -> Option<protocol::Price> {
    let raw = price.raw();
    let half = PRICE_UNIT / 2;
    let rounded = if raw < 0 {
        raw.saturating_sub(half)
    } else {
        raw.saturating_add(half)
    };
    i64::try_from(rounded / PRICE_UNIT)
        .ok()
        .map(protocol::Price::from_units)
}

/// The nearest unit, half up; `None` past an `i64`.
pub fn size_units(size: Quantity) -> Option<protocol::Quantity> {
    let rounded = size.raw().saturating_add(SIZE_UNIT / 2);
    i64::try_from(rounded / SIZE_UNIT)
        .ok()
        .map(protocol::Quantity::from_units)
}

/// `None`, logged, when the price or size doesn't fit in units.
pub fn trade_message(trade: &TradeTick) -> Option<Message> {
    let (Some(price), Some(size)) = (price_units(trade.price), size_units(trade.size)) else {
        log::warn!(
            "{} trade {} at {} dropped: out of range",
            trade.size,
            trade.instrument_id,
            trade.price
        );
        return None;
    };
    Some(Message::Trade(Trade {
        instrument: trade.instrument_id.to_string(),
        ts: trade.ts_event.as_u64(),
        price,
        size,
        aggressor: match trade.aggressor_side {
            AggressorSide::Buy => Some(Side::Buy),
            AggressorSide::Sell => Some(Side::Sell),
            AggressorSide::NoAggressor => None,
        },
        id: trade.trade_id.to_string(),
    }))
}

/// A batch as level updates; a clear starts over with an empty snapshot. A
/// level whose price or size doesn't fit in units is logged and dropped.
pub fn book_messages(deltas: &OrderBookDeltas) -> Vec<Message> {
    let instrument = deltas.instrument_id.to_string();
    let ts = deltas.ts_event.as_u64();
    let message = |data| {
        Message::Book(BookUpdate {
            instrument: instrument.clone(),
            ts,
            data,
        })
    };
    let mut messages = Vec::new();
    let mut levels = Vec::new();
    for delta in &deltas.deltas {
        if delta.action == BookAction::Clear {
            if !levels.is_empty() {
                messages.push(message(BookData::Update {
                    levels: std::mem::take(&mut levels),
                }));
            }
            messages.push(message(BookData::Snapshot {
                bids: Vec::new(),
                asks: Vec::new(),
            }));
            continue;
        }
        let book_side = match delta.order.side {
            Some(OrderSide::Buy) => BookSide::Bid,
            Some(OrderSide::Sell) => BookSide::Ask,
            None => continue,
        };
        let size = match delta.action {
            BookAction::Delete => Some(protocol::Quantity::ZERO),
            _ => size_units(delta.order.size),
        };
        let (Some(price), Some(size)) = (price_units(delta.order.price), size) else {
            log::warn!(
                "{} level of {} at {} dropped: out of range",
                delta.order.size,
                deltas.instrument_id,
                delta.order.price
            );
            continue;
        };
        levels.push((book_side, price, size));
    }
    if !levels.is_empty() {
        messages.push(message(BookData::Update { levels }));
    }
    messages
}

#[cfg(test)]
mod tests {
    use nautilus_model::{
        data::{BookOrder, OrderBookDelta},
        identifiers::TradeId,
        types::{price::PRICE_RAW_MAX, quantity::QUANTITY_RAW_MAX},
    };

    use super::*;

    const ID: &str = "BTC-PERP.SVP";

    fn delta(action: BookAction, side: OrderSide, price: &str, quantity: &str) -> OrderBookDelta {
        let order = BookOrder::new(side, Price::from(price), Quantity::from(quantity), 0);
        OrderBookDelta::new(
            InstrumentId::from(ID),
            action,
            order,
            0,
            1,
            5.into(),
            5.into(),
        )
    }

    fn px(s: &str) -> protocol::Price {
        s.parse().unwrap()
    }

    fn qty(s: &str) -> protocol::Quantity {
        s.parse().unwrap()
    }

    fn update(levels: &[(BookSide, &str, &str)]) -> Message {
        let levels = levels
            .iter()
            .map(|&(side, price, size)| (side, px(price), qty(size)))
            .collect();
        Message::Book(BookUpdate {
            instrument: ID.into(),
            ts: 5,
            data: BookData::Update { levels },
        })
    }

    #[test]
    fn converts_a_trade() {
        let trade = TradeTick::new(
            InstrumentId::from(ID),
            Price::from("83471.5"),
            Quantity::from("0.25"),
            AggressorSide::Sell,
            TradeId::from("abc-BIN"),
            7.into(),
            7.into(),
        );
        assert_eq!(
            trade_message(&trade).unwrap(),
            Message::Trade(Trade {
                instrument: ID.into(),
                ts: 7,
                price: px("83471.5"),
                size: qty("0.25"),
                aggressor: Some(Side::Sell),
                id: "abc-BIN".into(),
            })
        );
    }

    #[test]
    fn converts_deltas_to_level_updates() {
        let deltas = OrderBookDeltas::new(
            InstrumentId::from(ID),
            vec![
                delta(BookAction::Add, OrderSide::Buy, "100.0", "1.5"),
                delta(BookAction::Update, OrderSide::Sell, "101.0", "2"),
                delta(BookAction::Delete, OrderSide::Buy, "99.0", "0"),
            ],
        );
        assert_eq!(
            book_messages(&deltas),
            [update(&[
                (BookSide::Bid, "100.0", "1.5"),
                (BookSide::Ask, "101.0", "2"),
                (BookSide::Bid, "99.0", "0"),
            ])]
        );
    }

    #[test]
    fn a_clear_starts_over_with_an_empty_snapshot() {
        let clear = OrderBookDelta::clear(InstrumentId::from(ID), 1, 5.into(), 5.into());
        let deltas = OrderBookDeltas::new(
            InstrumentId::from(ID),
            vec![clear, delta(BookAction::Add, OrderSide::Buy, "100.0", "1")],
        );
        let empty = Message::Book(BookUpdate {
            instrument: ID.into(),
            ts: 5,
            data: BookData::Snapshot {
                bids: vec![],
                asks: vec![],
            },
        });
        assert_eq!(
            book_messages(&deltas),
            [empty, update(&[(BookSide::Bid, "100.0", "1")])]
        );
    }

    fn units(price: &str) -> Option<protocol::Price> {
        price_units(Price::from(price))
    }

    fn size(quantity: &str) -> Option<protocol::Quantity> {
        size_units(Quantity::from(quantity))
    }

    #[test]
    fn keeps_values_within_the_scale_exactly() {
        assert_eq!(units("83470.9"), Some(px("83470.9")));
        assert_eq!(units("83470.90000000000"), Some(px("83470.9")));
        assert_eq!(units("-0.00000000001"), Some(px("-0.00000000001")));
        assert_eq!(size("0.00000001"), Some(qty("0.00000001")));
        assert_eq!(size("3"), Some(qty("3")));
    }

    // Prices converted to USD through a stablecoin rate get extra decimals.
    #[test]
    fn rounds_values_past_the_scale_to_the_nearest_unit() {
        assert_eq!(units("1.0000000000049"), Some(px("1")));
        assert_eq!(units("1.000000000005"), Some(px("1.00000000001")));
        assert_eq!(units("-1.000000000005"), Some(px("-1.00000000001")));
        assert_eq!(units("-1.0000000000049"), Some(px("-1")));
        assert_eq!(size("0.123456784999"), Some(qty("0.12345678")));
        assert_eq!(size("0.123456785"), Some(qty("0.12345679")));
    }

    #[test]
    fn values_past_an_i64_are_none() {
        assert_eq!(
            units("92233720.36854775807"),
            Some(protocol::Price::from_units(i64::MAX))
        );
        assert_eq!(units("92233720.36854775808"), None);
        assert_eq!(units("92233720.368547758075"), None);
        assert_eq!(
            units("-92233720.36854775808"),
            Some(protocol::Price::from_units(i64::MIN))
        );
        assert_eq!(units("-92233720.36854775809"), None);
        assert_eq!(
            size("92233720368.54775807"),
            Some(protocol::Quantity::from_units(i64::MAX))
        );
        assert_eq!(size("92233720368.54775808"), None);
    }

    // At precision 16 the largest Nautilus values need 30 digits; none fit.
    #[test]
    fn the_largest_values_are_none_at_every_precision() {
        for precision in 0..=FIXED_PRECISION {
            let price = Price::from_raw(PRICE_RAW_MAX, precision);
            assert_eq!(price_units(price), None, "price at precision {precision}");
            let price = Price::from_raw(-PRICE_RAW_MAX, precision);
            assert_eq!(price_units(price), None, "price at precision {precision}");
            let size = Quantity::from_raw(QUANTITY_RAW_MAX, precision);
            assert_eq!(size_units(size), None, "size at precision {precision}");
        }
    }

    #[test]
    fn an_out_of_range_trade_or_level_is_dropped() {
        let trade = TradeTick::new(
            InstrumentId::from(ID),
            Price::from("100000000"),
            Quantity::from("1"),
            AggressorSide::Buy,
            TradeId::from("abc-BIN"),
            7.into(),
            7.into(),
        );
        assert_eq!(trade_message(&trade), None);

        let deltas = OrderBookDeltas::new(
            InstrumentId::from(ID),
            vec![
                delta(BookAction::Add, OrderSide::Sell, "100000000", "1"),
                delta(BookAction::Add, OrderSide::Buy, "100.0", "100000000000"),
                delta(BookAction::Add, OrderSide::Buy, "100.0", "1"),
            ],
        );
        assert_eq!(
            book_messages(&deltas),
            [update(&[(BookSide::Bid, "100.0", "1")])]
        );
    }
}
