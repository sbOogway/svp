//! Where unified data goes once it leaves the node: the [`Publisher`] turns
//! it into protocol [`Message`]s and hands them to each [`Sink`]. A transport
//! is a sink, or reads from [`ChannelSink`]; the node doesn't know which.

use std::fmt::Debug;

use nautilus_common::{
    actor::{DataActor, DataActorConfig, DataActorCore},
    nautilus_actor,
};
use nautilus_model::{
    data::{OrderBookDeltas, TradeTick},
    enums::{AggressorSide, BookAction, BookType, OrderSide},
    identifiers::{ActorId, InstrumentId},
};
use svp_protocol::{BookData, BookSide, BookUpdate, Message, Side, Trade};
use tokio::sync::broadcast;

pub trait Sink: Debug {
    fn send(&mut self, message: &Message);
}

/// Logs every message at debug level.
#[derive(Debug, Default)]
pub struct LogSink;

impl Sink for LogSink {
    fn send(&mut self, message: &Message) {
        match message {
            Message::Trade(t) => log::debug!(
                "trade {} {} {} @ {} id={}",
                t.instrument,
                match t.aggressor {
                    Some(Side::Buy) => "buy",
                    Some(Side::Sell) => "sell",
                    None => "-",
                },
                t.size,
                t.price,
                t.id
            ),
            Message::Book(b) => match &b.data {
                BookData::Snapshot { bids, asks } => log::debug!(
                    "book {} snapshot, {} bids, {} asks",
                    b.instrument,
                    bids.len(),
                    asks.len()
                ),
                BookData::Update { levels } => {
                    for (side, price, size) in levels {
                        log::debug!("book {} {side:?} {price} {size}", b.instrument);
                    }
                }
            },
        }
    }
}

/// Hands messages to other threads over a broadcast channel: the node runs
/// on one thread, a transport usually on others.
#[derive(Debug)]
pub struct ChannelSink {
    tx: broadcast::Sender<Message>,
}

impl ChannelSink {
    /// The sink, and a sender to subscribe receivers from. A receiver that
    /// falls more than `capacity` messages behind loses the oldest ones.
    pub fn new(capacity: usize) -> (Self, broadcast::Sender<Message>) {
        let (tx, _) = broadcast::channel(capacity);
        (Self { tx: tx.clone() }, tx)
    }
}

impl Sink for ChannelSink {
    fn send(&mut self, message: &Message) {
        // Fails only while nobody is subscribed.
        let _ = self.tx.send(message.clone());
    }
}

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
        price: trade.price.as_f64(),
        size: trade.size.as_f64(),
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
        let quantity = match delta.action {
            BookAction::Delete => 0.0,
            _ => delta.order.size.as_f64(),
        };
        levels.push((book_side, delta.order.price.as_f64(), quantity));
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
        types::{Price, Quantity},
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

    fn update(levels: Vec<(BookSide, f64, f64)>) -> Message {
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
                price: 83471.5,
                size: 0.25,
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
            [update(vec![
                (BookSide::Bid, 100.0, 1.5),
                (BookSide::Ask, 101.0, 2.0),
                (BookSide::Bid, 99.0, 0.0),
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
            [empty, update(vec![(BookSide::Bid, 100.0, 1.0)])]
        );
    }

    #[test]
    fn channel_sink_reaches_every_subscriber() {
        let (mut sink, tx) = ChannelSink::new(8);
        let (mut a, mut b) = (tx.subscribe(), tx.subscribe());
        let message = update(vec![(BookSide::Ask, 101.0, 1.0)]);
        sink.send(&message);
        assert_eq!(a.try_recv().unwrap(), message);
        assert_eq!(b.try_recv().unwrap(), message);
    }
}
