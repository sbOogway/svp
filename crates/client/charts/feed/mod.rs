//! What the app knows from the server: the instruments it offers, and for
//! each one the app subscribed to, the [`Book`], the last trade and how
//! fast messages arrive. [`connection`] turns the socket into [`Event`]s;
//! [`Feed`] keeps what they say.

pub mod connection;

use std::{
    collections::{BTreeMap, HashMap},
    time::{Duration, Instant},
};

use svp_common::protocol::{Book, Instrument, Message, Subscription, Trade};
use tokio::sync::mpsc;

pub use connection::connection;

#[derive(Debug, Clone)]
pub enum Event {
    Connected {
        session: u64,
        instruments: Vec<Instrument>,
        commands: Commands,
    },
    Received(Message),
    Disconnected {
        reason: String,
        retry_in: Duration,
    },
}

/// Sends requests down the connection an [`Event::Connected`] opened.
#[derive(Debug, Clone)]
pub struct Commands(mpsc::UnboundedSender<Command>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Subscribe(Vec<Subscription>),
    Unsubscribe(Vec<Subscription>),
}

impl Commands {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<Command>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self(sender), receiver)
    }

    /// A command sent after the connection dropped is lost with it: the
    /// next [`Event::Connected`] starts from nothing subscribed.
    fn send(&self, command: Command) {
        let _ = self.0.send(command);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Not connected yet, or waiting to retry after `last_error`.
    Connecting {
        last_error: Option<String>,
    },
    Connected {
        session: u64,
    },
}

/// The kinds of data a pane needs for its instrument.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Streams {
    pub trades: bool,
    pub books: bool,
}

impl Streams {
    pub const ALL: Self = Self {
        trades: true,
        books: true,
    };

    fn union(self, other: Self) -> Self {
        Self {
            trades: self.trades || other.trades,
            books: self.books || other.books,
        }
    }

    fn minus(self, other: Self) -> Self {
        Self {
            trades: self.trades && !other.trades,
            books: self.books && !other.books,
        }
    }

    fn is_empty(self) -> bool {
        !self.trades && !self.books
    }

    fn subscription(self, instrument: &str) -> Subscription {
        Subscription {
            instrument: instrument.to_owned(),
            trades: self.trades,
            books: self.books,
        }
    }
}

/// What the app keeps of one subscribed instrument.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Market {
    pub book: Book,
    /// When the book last changed, in UNIX nanoseconds.
    pub book_ts: u64,
    pub last_trade: Option<Trade>,
    /// Resyncs since subscribing: trades in each gap are lost.
    pub gaps: u64,
    pub missed: u64,
    rate: Rate,
}

impl Market {
    /// Messages per second over the last whole second.
    pub fn rate(&self) -> f32 {
        self.rate.per_second
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Rate {
    count: u32,
    since: Option<Instant>,
    per_second: f32,
}

impl Rate {
    fn sample(&mut self, now: Instant) {
        let Some(since) = self.since else {
            self.since = Some(now);
            return;
        };
        let elapsed = now.duration_since(since);
        if elapsed >= Duration::from_secs(1) {
            #[allow(clippy::cast_precision_loss)]
            let count = self.count as f32;
            self.per_second = count / elapsed.as_secs_f32();
            self.count = 0;
            self.since = Some(now);
        }
    }
}

#[derive(Debug)]
pub struct Feed {
    status: Status,
    instruments: Vec<Instrument>,
    commands: Option<Commands>,
    /// What the server was asked for on this connection.
    subscribed: BTreeMap<String, Streams>,
    markets: HashMap<String, Market>,
    /// Trades wait here for the next frame, so a burst costs one redraw.
    pending: Vec<Trade>,
    /// What the next [`Feed::tick`] hands to the panes.
    flushed: Vec<Trade>,
}

impl Default for Feed {
    fn default() -> Self {
        Self {
            status: Status::Connecting { last_error: None },
            instruments: Vec::new(),
            commands: None,
            subscribed: BTreeMap::new(),
            markets: HashMap::new(),
            pending: Vec::new(),
            flushed: Vec::new(),
        }
    }
}

impl Feed {
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// What the server's last welcome offered; kept while reconnecting.
    pub fn instruments(&self) -> &[Instrument] {
        &self.instruments
    }

    pub fn market(&self, instrument: &str) -> Option<&Market> {
        self.markets.get(instrument)
    }

    pub fn instrument(&self, id: &str) -> Option<&Instrument> {
        self.instruments.iter().find(|i| i.id == id)
    }

    pub fn offers(&self, instrument: &str) -> bool {
        self.instrument(instrument).is_some()
    }

    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Connected {
                session,
                instruments,
                commands,
            } => {
                self.status = Status::Connected { session };
                self.instruments = instruments;
                self.commands = Some(commands);
                self.subscribed.clear();
                self.markets.clear();
                self.pending.clear();
                self.flushed.clear();
            }
            Event::Received(message) => self.receive(message),
            Event::Disconnected { reason, .. } => {
                self.status = Status::Connecting {
                    last_error: Some(reason),
                };
                self.commands = None;
                self.subscribed.clear();
                self.pending.clear();
                self.flushed.clear();
            }
        }
    }

    fn receive(&mut self, message: Message) {
        match message {
            Message::Trade(trade) => {
                if let Some(market) = self.markets.get_mut(&trade.instrument) {
                    market.rate.count += 1;
                    self.pending.push(trade);
                }
            }
            Message::Book(update) => {
                if let Some(market) = self.markets.get_mut(&update.instrument) {
                    market.rate.count += 1;
                    market.book.apply(&update.data);
                    market.book_ts = update.ts;
                }
            }
            Message::Resync { missed } => {
                self.flush();
                for market in self.markets.values_mut() {
                    market.gaps += 1;
                    market.missed += missed;
                }
            }
            Message::Welcome { .. }
            | Message::Error { .. }
            | Message::Reject { .. }
            | Message::Goodbye { .. } => {}
        }
    }

    /// Applies the trades received since the last frame, and returns them.
    pub fn tick(&mut self, now: Instant) -> Vec<Trade> {
        self.flush();
        for market in self.markets.values_mut() {
            market.rate.sample(now);
        }
        std::mem::take(&mut self.flushed)
    }

    fn flush(&mut self) {
        for trade in self.pending.drain(..) {
            if let Some(market) = self.markets.get_mut(&trade.instrument) {
                market.last_trade = Some(trade.clone());
                self.flushed.push(trade);
            }
        }
    }

    /// Subscribes to what `wanted` adds and unsubscribes from what it drops,
    /// among the instruments the server offers. Nothing is sent while
    /// disconnected; the next connection starts over from `wanted`.
    pub fn sync(&mut self, wanted: &BTreeMap<String, Streams>) {
        let Some(commands) = &self.commands else {
            return;
        };
        let wanted: BTreeMap<&str, Streams> = wanted
            .iter()
            .filter(|(id, streams)| !streams.is_empty() && self.offers(id))
            .map(|(id, &streams)| (id.as_str(), streams))
            .collect();

        let mut subscribe = Vec::new();
        let mut unsubscribe = Vec::new();
        for (id, &new) in &wanted {
            let old = self.subscribed.get(*id).copied().unwrap_or_default();
            let added = new.minus(old);
            if !added.is_empty() {
                subscribe.push(added.subscription(id));
            }
        }
        for (id, &old) in &self.subscribed {
            let new = wanted.get(id.as_str()).copied().unwrap_or_default();
            let dropped = old.minus(new);
            if !dropped.is_empty() {
                unsubscribe.push(dropped.subscription(id));
            }
        }

        if !unsubscribe.is_empty() {
            commands.send(Command::Unsubscribe(unsubscribe));
        }
        if !subscribe.is_empty() {
            commands.send(Command::Subscribe(subscribe));
        }

        self.subscribed = wanted
            .into_iter()
            .map(|(id, streams)| (id.to_owned(), streams))
            .collect();
        self.markets
            .retain(|id, _| self.subscribed.contains_key(id));
        for (id, streams) in &self.subscribed {
            let market = self.markets.entry(id.clone()).or_default();
            if !streams.books {
                market.book = Book::default();
                market.book_ts = 0;
            }
            if !streams.trades {
                market.last_trade = None;
            }
        }
        self.flushed.retain(|trade| {
            self.subscribed
                .get(&trade.instrument)
                .is_some_and(|s| s.trades)
        });
    }

    /// The union of what each of `wanted` needs, per instrument.
    pub fn merge<'a>(
        wanted: impl IntoIterator<Item = (&'a str, Streams)>,
    ) -> BTreeMap<String, Streams> {
        let mut merged = BTreeMap::<String, Streams>::new();
        for (id, streams) in wanted {
            let entry = merged.entry(id.to_owned()).or_default();
            *entry = entry.union(streams);
        }
        merged
    }
}

#[cfg(test)]
mod tests {
    use svp_common::{
        market::Coin,
        protocol::{BookData, BookSide, BookUpdate, Market as Kind, Price, Quantity},
    };

    use super::*;

    fn px(s: &str) -> Price {
        s.parse().unwrap()
    }

    fn qty(s: &str) -> Quantity {
        s.parse().unwrap()
    }

    pub(super) fn instrument(id: &str) -> Instrument {
        Instrument {
            id: id.into(),
            coin: Coin::BTC,
            market: Kind::Perp,
            venues: vec!["BINANCE".into()],
        }
    }

    pub(super) fn trade(instrument: &str, price: &str) -> Message {
        Message::Trade(Trade {
            instrument: instrument.into(),
            ts: 1,
            price: px(price),
            size: qty("0.1"),
            aggressor: None,
            id: "t".into(),
        })
    }

    pub(super) fn snapshot(instrument: &str, bid: &str, ask: &str) -> Message {
        Message::Book(BookUpdate {
            instrument: instrument.into(),
            ts: 1,
            data: BookData::Snapshot {
                bids: vec![(px(bid), qty("1"))],
                asks: vec![(px(ask), qty("1"))],
            },
        })
    }

    fn connected(ids: &[&str]) -> (Feed, mpsc::UnboundedReceiver<Command>) {
        let (commands, receiver) = Commands::new();
        let mut feed = Feed::default();
        feed.apply(Event::Connected {
            session: 1,
            instruments: ids.iter().map(|id| instrument(id)).collect(),
            commands,
        });
        (feed, receiver)
    }

    fn wanted(entries: &[(&str, Streams)]) -> BTreeMap<String, Streams> {
        Feed::merge(entries.iter().map(|&(id, s)| (id, s)))
    }

    fn drain(receiver: &mut mpsc::UnboundedReceiver<Command>) -> Vec<Command> {
        std::iter::from_fn(|| receiver.try_recv().ok()).collect()
    }

    const TRADES: Streams = Streams {
        trades: true,
        books: false,
    };

    #[test]
    fn sync_subscribes_to_what_panes_add_and_drops_what_they_leave() {
        let (mut feed, mut commands) = connected(&["A", "B"]);

        feed.sync(&wanted(&[
            ("A", TRADES),
            ("A", Streams::ALL),
            ("C", TRADES),
        ]));
        assert_eq!(
            drain(&mut commands),
            [Command::Subscribe(vec![Streams::ALL.subscription("A")])],
            "C isn't offered, and A's streams merge"
        );

        feed.sync(&wanted(&[("A", TRADES), ("B", TRADES)]));
        assert_eq!(
            drain(&mut commands),
            [
                Command::Unsubscribe(vec![Subscription {
                    instrument: "A".into(),
                    trades: false,
                    books: true,
                }]),
                Command::Subscribe(vec![TRADES.subscription("B")]),
            ]
        );

        feed.sync(&wanted(&[("A", TRADES), ("B", TRADES)]));
        assert_eq!(drain(&mut commands), []);

        feed.sync(&wanted(&[]));
        assert_eq!(
            drain(&mut commands),
            [Command::Unsubscribe(vec![
                TRADES.subscription("A"),
                TRADES.subscription("B"),
            ])]
        );
        assert!(feed.market("A").is_none());
    }

    #[test]
    fn a_new_connection_subscribes_again_from_nothing() {
        let (mut feed, _) = connected(&["A"]);
        feed.sync(&wanted(&[("A", Streams::ALL)]));

        feed.apply(Event::Disconnected {
            reason: "gone".into(),
            retry_in: Duration::ZERO,
        });
        assert_eq!(
            feed.status(),
            &Status::Connecting {
                last_error: Some("gone".into())
            }
        );
        feed.sync(&wanted(&[("A", Streams::ALL)]));

        let (commands, mut receiver) = Commands::new();
        feed.apply(Event::Connected {
            session: 2,
            instruments: vec![instrument("A")],
            commands,
        });
        feed.sync(&wanted(&[("A", Streams::ALL)]));
        assert_eq!(
            drain(&mut receiver),
            [Command::Subscribe(vec![Streams::ALL.subscription("A")])]
        );
    }

    #[test]
    fn trades_wait_for_the_frame() {
        let (mut feed, _) = connected(&["A"]);
        feed.sync(&wanted(&[("A", Streams::ALL)]));

        feed.apply(Event::Received(trade("A", "100")));
        feed.apply(Event::Received(trade("A", "101")));
        feed.apply(Event::Received(trade("B", "1")));
        assert_eq!(feed.market("A").unwrap().last_trade, None);

        let trades = feed.tick(Instant::now());
        let last = feed.market("A").unwrap().last_trade.as_ref().unwrap();
        assert_eq!(last.price, px("101"));
        assert!(feed.market("B").is_none());
        assert_eq!(
            trades.iter().map(|t| t.price).collect::<Vec<_>>(),
            [px("100"), px("101")]
        );
        assert!(feed.tick(Instant::now()).is_empty());
    }

    #[test]
    fn books_follow_snapshots_and_updates() {
        let (mut feed, _) = connected(&["A"]);
        feed.sync(&wanted(&[("A", Streams::ALL)]));

        feed.apply(Event::Received(snapshot("A", "99", "101")));
        feed.apply(Event::Received(Message::Book(BookUpdate {
            instrument: "A".into(),
            ts: 2,
            data: BookData::Update {
                levels: vec![
                    (BookSide::Bid, px("100"), qty("2")),
                    (BookSide::Ask, px("101"), qty("0")),
                    (BookSide::Ask, px("102"), qty("3")),
                ],
            },
        })));

        let market = feed.market("A").unwrap();
        assert_eq!(market.book_ts, 2);
        let book = &market.book;
        assert_eq!(book.best_bid(), Some((px("100"), qty("2"))));
        assert_eq!(book.best_ask(), Some((px("102"), qty("3"))));
    }

    #[test]
    fn a_resync_marks_a_gap_after_the_trades_before_it() {
        let (mut feed, _) = connected(&["A"]);
        feed.sync(&wanted(&[("A", Streams::ALL)]));

        feed.apply(Event::Received(trade("A", "100")));
        feed.apply(Event::Received(Message::Resync { missed: 7 }));

        let market = feed.market("A").unwrap();
        assert_eq!((market.gaps, market.missed), (1, 7));
        assert_eq!(market.last_trade.as_ref().unwrap().price, px("100"));
        assert_eq!(feed.tick(Instant::now()).len(), 1, "the panes still get it");
    }

    #[test]
    fn rate_counts_messages_over_each_second() {
        let (mut feed, _) = connected(&["A"]);
        feed.sync(&wanted(&[("A", Streams::ALL)]));
        let start = Instant::now();
        feed.tick(start);

        for _ in 0..3 {
            feed.apply(Event::Received(trade("A", "100")));
        }
        feed.apply(Event::Received(snapshot("A", "99", "101")));
        feed.tick(start + Duration::from_millis(500));
        assert!(feed.market("A").unwrap().rate().abs() < f32::EPSILON);

        feed.tick(start + Duration::from_secs(2));
        assert!((feed.market("A").unwrap().rate() - 2.0).abs() < f32::EPSILON);
    }
}
