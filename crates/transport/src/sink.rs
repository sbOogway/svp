//! Where unified data goes once it leaves the aggregator. A transport is a
//! sink, or subscribes to [`ChannelSink`] through its [`Hub`]; the aggregator
//! doesn't know which.

use std::{
    collections::BTreeMap,
    fmt::Debug,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use svp_wire::{Book, BookData, BookUpdate, Message, Side};
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
            Message::Resync { missed } => log::debug!("resync after {missed} missed"),
        }
    }
}

/// Hands messages to other threads over a broadcast channel: the aggregator
/// runs on one thread, transports on others. It also keeps every book, so a
/// subscriber starts from snapshots.
#[derive(Debug)]
pub struct ChannelSink {
    hub: Hub,
}

impl ChannelSink {
    /// The sink, and the hub to subscribe from. A receiver that falls more
    /// than `capacity` messages behind loses the oldest ones.
    pub fn new(capacity: usize) -> (Self, Hub) {
        let hub = Hub {
            shared: Arc::new(Mutex::new(Shared {
                tx: broadcast::channel(capacity).0,
                books: BTreeMap::new(),
            })),
        };
        (Self { hub: hub.clone() }, hub)
    }
}

impl Sink for ChannelSink {
    fn send(&mut self, message: &Message) {
        let mut shared = self.hub.lock();
        if let Message::Book(update) = message {
            let (ts, book) = shared.books.entry(update.instrument.clone()).or_default();
            *ts = update.ts;
            book.apply(&update.data);
        }
        // Fails only while nobody is subscribed.
        let _ = shared.tx.send(message.clone());
    }
}

#[derive(Debug, Clone)]
pub struct Hub {
    shared: Arc<Mutex<Shared>>,
}

#[derive(Debug)]
struct Shared {
    tx: broadcast::Sender<Message>,
    /// By instrument, with the timestamp of the last update.
    books: BTreeMap<String, (u64, Book)>,
}

impl Hub {
    /// A snapshot of every book and a receiver of what comes after them.
    /// The sink applies and broadcasts under the same lock, so the receiver
    /// starts exactly where the snapshots end.
    pub fn subscribe(&self) -> (Vec<Message>, broadcast::Receiver<Message>) {
        let shared = self.lock();
        let snapshots = shared
            .books
            .iter()
            .map(|(instrument, (ts, book))| {
                Message::Book(BookUpdate {
                    instrument: instrument.clone(),
                    ts: *ts,
                    data: book.snapshot(),
                })
            })
            .collect();
        (snapshots, shared.tx.subscribe())
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use svp_wire::{BookSide, Trade};

    use super::*;

    pub(crate) const ID: &str = "BTC-PERP.SVP";

    pub(crate) fn update(ts: u64, levels: Vec<(BookSide, f64, f64)>) -> Message {
        Message::Book(BookUpdate {
            instrument: ID.into(),
            ts,
            data: BookData::Update { levels },
        })
    }

    pub(crate) fn trade(ts: u64) -> Message {
        Message::Trade(Trade {
            instrument: ID.into(),
            ts,
            price: 100.0,
            size: 1.0,
            aggressor: Some(Side::Buy),
            id: ts.to_string(),
        })
    }

    #[test]
    fn channel_sink_reaches_every_subscriber() {
        let (mut sink, hub) = ChannelSink::new(8);
        let ((_, mut a), (_, mut b)) = (hub.subscribe(), hub.subscribe());
        let message = update(1, vec![(BookSide::Ask, 101.0, 1.0)]);
        sink.send(&message);
        assert_eq!(a.try_recv().unwrap(), message);
        assert_eq!(b.try_recv().unwrap(), message);
    }

    #[test]
    fn a_late_subscriber_starts_from_snapshots() {
        let (mut sink, hub) = ChannelSink::new(8);
        sink.send(&update(1, vec![(BookSide::Bid, 100.0, 1.0)]));
        sink.send(&trade(2));
        sink.send(&update(3, vec![(BookSide::Ask, 101.0, 2.0)]));

        let (snapshots, mut rx) = hub.subscribe();
        assert_eq!(
            snapshots,
            [Message::Book(BookUpdate {
                instrument: ID.into(),
                ts: 3,
                data: BookData::Snapshot {
                    bids: vec![(100.0, 1.0)],
                    asks: vec![(101.0, 2.0)],
                },
            })]
        );
        assert!(rx.try_recv().is_err());

        sink.send(&trade(4));
        assert_eq!(rx.try_recv().unwrap(), trade(4));
    }
}
