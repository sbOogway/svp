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
};
use svp_transport::sink::Sink;
use svp_wire::{BookData, BookSide, BookUpdate, Message, Side, Trade};

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
        self.send(&trade_message(trade));
        Ok(())
    }

    fn on_book_deltas(&mut self, deltas: &OrderBookDeltas) -> anyhow::Result<()> {
        for message in book_messages(deltas) {
            self.send(&message);
        }
        Ok(())
    }
}

pub fn trade_message(trade: &TradeTick) -> Message {
    Message::Trade(Trade {
        instrument: trade.instrument_id.to_string(),
        ts: trade.ts_event.as_u64(),
        price: trade.price.as_decimal().into(),
        size: trade.size.as_decimal().into(),
        aggressor: match trade.aggressor_side {
            AggressorSide::Buy => Some(Side::Buy),
            AggressorSide::Sell => Some(Side::Sell),
            AggressorSide::NoAggressor => None,
        },
        id: trade.trade_id.to_string(),
    })
}

/// A batch as level updates; a clear starts over with an empty snapshot.
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
            BookAction::Delete => svp_wire::Quantity::ZERO,
            _ => delta.order.size.as_decimal().into(),
        };
        levels.push((book_side, delta.order.price.as_decimal().into(), size));
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
        types::{
            Price, Quantity, fixed::FIXED_PRECISION, price::PRICE_RAW_MAX,
            quantity::QUANTITY_RAW_MAX,
        },
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

    fn px(s: &str) -> svp_wire::Price {
        s.parse().unwrap()
    }

    fn qty(s: &str) -> svp_wire::Quantity {
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
            trade_message(&trade),
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

    #[test]
    fn keeps_trailing_zeros() {
        let trade = TradeTick::new(
            InstrumentId::from(ID),
            Price::from("83470.900"),
            Quantity::from("0.30000000"),
            AggressorSide::NoAggressor,
            TradeId::from("abc-BIN"),
            7.into(),
            7.into(),
        );
        let Message::Trade(converted) = trade_message(&trade) else {
            panic!("expected a trade");
        };
        assert_eq!(converted.price.to_string(), "83470.900");
        assert_eq!(converted.size.to_string(), "0.30000000");
    }

    /// `digits` with a decimal point `precision` digits from the right.
    fn with_point(digits: &str, precision: u8) -> String {
        let precision = usize::from(precision);
        let digits = format!("{digits:0>width$}", width = precision + 1);
        let (whole, fraction) = digits.split_at(digits.len() - precision);
        if fraction.is_empty() {
            whole.to_string()
        } else {
            format!("{whole}.{fraction}")
        }
    }

    // One below the largest, so every digit is significant. At precision 16
    // those need 30 digits, more than a `Decimal` holds.
    #[test]
    fn converts_the_largest_values_exactly_up_to_precision_15() {
        for precision in 0..FIXED_PRECISION {
            let unit = 10_i128.pow(u32::from(FIXED_PRECISION - precision));
            let raw = PRICE_RAW_MAX / unit * unit - unit;
            assert_eq!(
                svp_wire::Price::from(Price::from_raw(raw, precision).as_decimal()).to_string(),
                with_point(&(raw / unit).to_string(), precision),
                "price at precision {precision}"
            );
            let unit = 10_u128.pow(u32::from(FIXED_PRECISION - precision));
            let raw = QUANTITY_RAW_MAX / unit * unit - unit;
            assert_eq!(
                svp_wire::Quantity::from(Quantity::from_raw(raw, precision).as_decimal())
                    .to_string(),
                with_point(&(raw / unit).to_string(), precision),
                "size at precision {precision}"
            );
        }
    }
}
