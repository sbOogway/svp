//! Where unified data goes once it leaves the aggregator. A transport is a
//! sink, or subscribes to [`ChannelSink`] through its [`Hub`]; the aggregator
//! doesn't know which.

use std::{
    collections::BTreeMap,
    fmt::Debug,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use svp_wire::{Book, BookData, BookUpdate, Instrument, Message, Side};
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
            Message::Welcome { .. }
            | Message::Error { .. }
            | Message::Reject { .. }
            | Message::Goodbye { .. } => {
                log::debug!("{message:?}");
            }
        }
    }
}

/// Hands messages to other threads over a broadcast channel: the aggregator
/// runs on one thread, transports on others. It also keeps every book, so a
/// subscriber starts from snapshots, and the instruments clients can pick.
#[derive(Debug)]
pub struct ChannelSink {
    hub: Hub,
}

impl ChannelSink {
    /// The sink, and the hub to subscribe from. A receiver that falls more
    /// than `capacity` messages behind loses the oldest ones.
    pub fn new(capacity: usize, instruments: Vec<Instrument>) -> (Self, Hub) {
        let hub = Hub {
            shared: Arc::new(Mutex::new(Shared {
                tx: broadcast::channel(capacity).0,
                books: BTreeMap::new(),
            })),
            instruments: instruments.into(),
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
    instruments: Arc<[Instrument]>,
}

#[derive(Debug)]
struct Shared {
    tx: broadcast::Sender<Message>,
    /// By instrument, with the timestamp of the last update.
    books: BTreeMap<String, (u64, Book)>,
}

impl Hub {
    pub fn instruments(&self) -> &[Instrument] {
        &self.instruments
    }

    /// A snapshot of each book `wants` names and a receiver of what comes
    /// after them. The sink applies and broadcasts under the same lock, so
    /// the receiver starts exactly where the snapshots end.
    pub fn subscribe(
        &self,
        wants: impl Fn(&str) -> bool,
    ) -> (Vec<Message>, broadcast::Receiver<Message>) {
        let shared = self.lock();
        (shared.snapshots(wants), shared.tx.subscribe())
    }

    /// Snapshots for `rx` to pick more books up from, and how many messages
    /// `rx` has queued from before them: updates among those are already in
    /// the snapshots.
    pub fn catch_up(
        &self,
        rx: &broadcast::Receiver<Message>,
        wants: impl Fn(&str) -> bool,
    ) -> (Vec<Message>, usize) {
        let shared = self.lock();
        (shared.snapshots(wants), rx.len())
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Shared {
    fn snapshots(&self, wants: impl Fn(&str) -> bool) -> Vec<Message> {
        self.books
            .iter()
            .filter(|(instrument, _)| wants(instrument))
            .map(|(instrument, (ts, book))| {
                Message::Book(BookUpdate {
                    instrument: instrument.clone(),
                    ts: *ts,
                    data: book.snapshot(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use svp_wire::{BookSide, Market, Price, Quantity, Subscription, Trade};

    use super::*;

    pub(crate) const ID: &str = "BTC-PERP.SVP";
    pub(crate) const OTHER: &str = "ETH-PERP.SVP";

    /// A [`ChannelSink`] offering [`ID`] and [`OTHER`].
    pub(crate) fn channel(capacity: usize) -> (ChannelSink, Hub) {
        let instrument = |id: &str, coin: &str| Instrument {
            id: id.into(),
            coin: coin.into(),
            market: Market::Perp,
            venues: vec!["BINANCE".into()],
        };
        ChannelSink::new(
            capacity,
            vec![instrument(ID, "BTC"), instrument(OTHER, "ETH")],
        )
    }

    pub(crate) fn subscription(instrument: &str, trades: bool, books: bool) -> Subscription {
        Subscription {
            instrument: instrument.into(),
            trades,
            books,
        }
    }

    pub(crate) fn px(s: &str) -> Price {
        s.parse().unwrap()
    }

    pub(crate) fn qty(s: &str) -> Quantity {
        s.parse().unwrap()
    }

    pub(crate) fn update(ts: u64, levels: &[(BookSide, &str, &str)]) -> Message {
        let levels = levels
            .iter()
            .map(|&(side, price, size)| (side, px(price), qty(size)))
            .collect();
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
            price: px("100"),
            size: qty("1"),
            aggressor: Some(Side::Buy),
            id: ts.to_string(),
        })
    }

    #[test]
    fn channel_sink_reaches_every_subscriber() {
        let (mut sink, hub) = channel(8);
        let ((_, mut a), (_, mut b)) = (hub.subscribe(|_| true), hub.subscribe(|_| true));
        let message = update(1, &[(BookSide::Ask, "101", "1")]);
        sink.send(&message);
        assert_eq!(a.try_recv().unwrap(), message);
        assert_eq!(b.try_recv().unwrap(), message);
    }

    #[test]
    fn a_late_subscriber_starts_from_snapshots() {
        let (mut sink, hub) = channel(8);
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        sink.send(&trade(2));
        sink.send(&update(3, &[(BookSide::Ask, "101", "2")]));

        let (snapshots, mut rx) = hub.subscribe(|_| true);
        assert_eq!(
            snapshots,
            [Message::Book(BookUpdate {
                instrument: ID.into(),
                ts: 3,
                data: BookData::Snapshot {
                    bids: vec![(px("100"), qty("1"))],
                    asks: vec![(px("101"), qty("2"))],
                },
            })]
        );
        assert!(rx.try_recv().is_err());

        sink.send(&trade(4));
        assert_eq!(rx.try_recv().unwrap(), trade(4));
    }
}
