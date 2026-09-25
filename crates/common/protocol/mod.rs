//! What the svp server and its clients say to each other: trades, book
//! updates and the [`Book`] both sides rebuild from them, and the connection
//! scheme every transport carries the same way.
//!
//! A client opens with [`Request::Hello`]; the server answers
//! [`Message::Welcome`] with the instruments it has, or [`Message::Reject`]
//! and closes. The client then subscribes to what it wants from that list,
//! and either side ends with a `Goodbye`.
//!
//! ```text
//! client                                        server
//!   |-- Hello {version, name} -------------------->|
//!   |<--------------------------- Reject {reason} -|  can't serve it, closes
//!   |<--------- Welcome {session, instruments} ----|
//!   |                                              |
//!   |-- Subscribe {subscriptions} ---------------->|
//!   |<---------------------------- Error {reason} -|  unknown instruments
//!   |<------------------------- Book (snapshot) ---|  per newly received book
//!   |<--------------------- Trade, Book (update) --|  ...
//!   |<--------------------------- Resync {missed} -|  client fell behind,
//!   |<------------------------- Book (snapshot) ---|  books start over
//!   |-- Unsubscribe {subscriptions} -------------->|
//!   |                                              |
//!   |-- Goodbye {reason} ------------------------->|  either side ends it
//!   |<-------------------------- Goodbye {reason} -|
//! ```
//!
//! Each one travels as a frame of `MessagePack` ([`encode`], [`decode`]);
//! how frames are carried is up to the transport.

mod codec;
mod unit;

use std::collections::BTreeMap;

pub use codec::{DecodeError, decode, encode};
use serde::{Deserialize, Serialize};
pub use unit::{Fixed, ParseUnitsError, Price, PriceStep, Quantity};

use crate::market::Coin;

/// Identifies a bar stream as `venue:symbol:timeframe`, e.g. `BINANCE:BTCUSDT-PERP:1m`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StreamId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Trade(Trade),
    Book(BookUpdate),
    /// The client fell behind and `missed` messages were dropped. A snapshot
    /// of every book follows; trades in the gap are lost.
    Resync {
        missed: u64,
    },
    /// Accepts a [`Request::Hello`]; the server's logs know this client by
    /// `session`. Nothing streams until the client subscribes to some of
    /// `instruments`.
    Welcome {
        session: u64,
        version: Version,
        instruments: Vec<Instrument>,
    },
    /// A request the server could not fully honor; the session goes on.
    Error {
        reason: String,
    },
    /// Refuses a [`Request::Hello`], then the server closes.
    Reject {
        reason: String,
    },
    /// The server is closing the connection.
    Goodbye {
        reason: String,
    },
}

/// What a client sends to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// The first frame of every connection.
    Hello { version: Version, name: String },
    /// Adds to what the client receives; a book it newly receives starts
    /// with a snapshot. An instrument the [`Message::Welcome`] did not list
    /// is answered with [`Message::Error`] and the rest still applies.
    Subscribe { subscriptions: Vec<Subscription> },
    /// Takes the kinds a subscription sets away from what the client
    /// receives for its instrument.
    Unsubscribe { subscriptions: Vec<Subscription> },
    /// The client is closing the connection.
    Goodbye { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub instrument: String,
    pub trades: bool,
    pub books: bool,
}

/// One instrument the server merges from many venues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instrument {
    /// What [`Trade::instrument`], [`BookUpdate::instrument`] and
    /// [`Subscription::instrument`] name it by.
    pub id: String,
    /// Its prices and sizes are shown with the coin's
    /// [`price_decimals`](Coin::price_decimals) and
    /// [`size_decimals`](Coin::size_decimals).
    pub coin: Coin,
    pub market: Market,
    pub venues: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Market {
    Spot,
    Perp,
}

/// The version of this crate's scheme. A minor bump only adds to it, so a
/// server serves clients of its major and any minor up to its own.
pub const PROTOCOL_VERSION: Version = Version { major: 2, minor: 0 };

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

impl Version {
    pub fn serves(self, client: Self) -> bool {
        self.major == client.major && client.minor <= self.minor
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Prices in USD, sizes in coins, timestamps in UNIX nanoseconds. Prices
/// and sizes are exact, as integer [units](Price::units) on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub instrument: String,
    pub ts: u64,
    pub price: Price,
    pub size: Quantity,
    /// `None` when the venue doesn't say which side took liquidity.
    pub aggressor: Option<Side>,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookUpdate {
    pub instrument: String,
    pub ts: u64,
    #[serde(flatten)]
    pub data: BookData,
}

/// A whole book, or the levels that changed since the last message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BookData {
    /// Bids best (highest) first, asks best (lowest) first, as `[price, size]`.
    Snapshot {
        bids: Vec<(Price, Quantity)>,
        asks: Vec<(Price, Quantity)>,
    },
    /// `[side, price, size]`; size 0 removes the level.
    Update {
        levels: Vec<(BookSide, Price, Quantity)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookSide {
    Bid,
    Ask,
}

/// A book kept from [`BookData`]: a server answers a late client with its
/// snapshot, and the app keeps one to draw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Book {
    bids: BTreeMap<Price, Quantity>,
    asks: BTreeMap<Price, Quantity>,
}

impl Book {
    pub fn apply(&mut self, data: &BookData) {
        match data {
            BookData::Snapshot { bids, asks } => {
                self.bids = bids.iter().copied().collect();
                self.asks = asks.iter().copied().collect();
            }
            BookData::Update { levels } => {
                for &(side, price, size) in levels {
                    let book = match side {
                        BookSide::Bid => &mut self.bids,
                        BookSide::Ask => &mut self.asks,
                    };
                    if size.units == 0 {
                        book.remove(&price);
                    } else {
                        book.insert(price, size);
                    }
                }
            }
        }
    }

    pub fn best_bid(&self) -> Option<(Price, Quantity)> {
        self.bids.last_key_value().map(|(&p, &s)| (p, s))
    }

    pub fn best_ask(&self) -> Option<(Price, Quantity)> {
        self.asks.first_key_value().map(|(&p, &s)| (p, s))
    }

    pub fn snapshot(&self) -> BookData {
        BookData::Snapshot {
            bids: self.bids.iter().rev().map(|(&p, &s)| (p, s)).collect(),
            asks: self.asks.iter().map(|(&p, &s)| (p, s)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(s: &str) -> Price {
        s.parse().unwrap()
    }

    fn qty(s: &str) -> Quantity {
        s.parse().unwrap()
    }

    #[test]
    fn stream_id_is_transparent_string() {
        let id = StreamId("BINANCE:BTCUSDT-PERP:1m".into());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"BINANCE:BTCUSDT-PERP:1m\"");
    }

    fn roundtrip(message: &Message, json: &str) {
        assert_eq!(serde_json::to_string(message).unwrap(), json);
        assert_eq!(&serde_json::from_str::<Message>(json).unwrap(), message);
    }

    #[test]
    fn trade_on_the_wire() {
        roundtrip(
            &Message::Trade(Trade {
                instrument: "BTC-PERP.SVP".into(),
                ts: 1,
                price: px("83471.5"),
                size: qty("0.250"),
                aggressor: Some(Side::Sell),
                id: "96728d723fa5443eadbaa76d67f9515a-CBS".into(),
            }),
            r#"{"type":"trade","instrument":"BTC-PERP.SVP","ts":1,"price":8347150000000000,"size":25000000,"aggressor":"sell","id":"96728d723fa5443eadbaa76d67f9515a-CBS"}"#,
        );
    }

    #[test]
    fn book_on_the_wire() {
        roundtrip(
            &Message::Book(BookUpdate {
                instrument: "BTC-PERP.SVP".into(),
                ts: 1,
                data: BookData::Snapshot {
                    bids: vec![(px("83471.0"), qty("1.2"))],
                    asks: vec![(px("83472.0"), qty("0.5"))],
                },
            }),
            r#"{"type":"book","instrument":"BTC-PERP.SVP","ts":1,"kind":"snapshot","bids":[[8347100000000000,120000000]],"asks":[[8347200000000000,50000000]]}"#,
        );
        roundtrip(
            &Message::Book(BookUpdate {
                instrument: "BTC-PERP.SVP".into(),
                ts: 2,
                data: BookData::Update {
                    levels: vec![
                        (BookSide::Bid, px("83470.9"), qty("0.4")),
                        (BookSide::Ask, px("83472.0"), qty("0")),
                    ],
                },
            }),
            r#"{"type":"book","instrument":"BTC-PERP.SVP","ts":2,"kind":"update","levels":[["bid",8347090000000000,40000000],["ask",8347200000000000,0]]}"#,
        );
    }

    #[test]
    fn resync_on_the_wire() {
        roundtrip(
            &Message::Resync { missed: 3 },
            r#"{"type":"resync","missed":3}"#,
        );
    }

    #[test]
    fn handshake_on_the_wire() {
        fn request(request: &Request, json: &str) {
            assert_eq!(serde_json::to_string(request).unwrap(), json);
            assert_eq!(&serde_json::from_str::<Request>(json).unwrap(), request);
        }

        request(
            &Request::Hello {
                version: PROTOCOL_VERSION,
                name: "tail".into(),
            },
            r#"{"type":"hello","version":{"major":2,"minor":0},"name":"tail"}"#,
        );
        roundtrip(
            &Message::Welcome {
                session: 7,
                version: PROTOCOL_VERSION,
                instruments: vec![Instrument {
                    id: "BTC-PERP.SVP".into(),
                    coin: Coin::BTC,
                    market: Market::Perp,
                    venues: vec!["BINANCE".into(), "OKX".into()],
                }],
            },
            r#"{"type":"welcome","session":7,"version":{"major":2,"minor":0},"instruments":[{"id":"BTC-PERP.SVP","coin":"BTC","market":"perp","venues":["BINANCE","OKX"]}]}"#,
        );
        request(
            &Request::Subscribe {
                subscriptions: vec![Subscription {
                    instrument: "BTC-PERP.SVP".into(),
                    trades: true,
                    books: false,
                }],
            },
            r#"{"type":"subscribe","subscriptions":[{"instrument":"BTC-PERP.SVP","trades":true,"books":false}]}"#,
        );
    }

    #[test]
    fn a_server_serves_its_major_up_to_its_minor() {
        let server = Version { major: 1, minor: 2 };
        assert!(server.serves(Version { major: 1, minor: 0 }));
        assert!(server.serves(Version { major: 1, minor: 2 }));
        assert!(!server.serves(Version { major: 1, minor: 3 }));
        assert!(!server.serves(Version { major: 0, minor: 2 }));
        assert!(!server.serves(Version { major: 2, minor: 0 }));
    }

    #[test]
    fn message_pack_carries_units_as_integers() {
        #[derive(Deserialize)]
        struct Units {
            price: i64,
            size: i64,
        }
        let message = Message::Trade(Trade {
            instrument: "BTC-PERP.SVP".into(),
            ts: 1,
            price: px("83470.9"),
            size: qty("0.1") + qty("0.2"),
            aggressor: None,
            id: "abc-BIN".into(),
        });
        let bytes = rmp_serde::to_vec_named(&message).unwrap();
        let units: Units = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(units.price, 8_347_090_000_000_000);
        assert_eq!(units.size, 30_000_000);
        assert_eq!(rmp_serde::from_slice::<Message>(&bytes).unwrap(), message);
    }

    #[test]
    fn book_keeps_updates_and_snapshots_them_best_first() {
        let mut book = Book::default();
        book.apply(&BookData::Update {
            levels: vec![
                (BookSide::Bid, px("99"), qty("1")),
                (BookSide::Bid, px("100"), qty("2")),
                (BookSide::Ask, px("102"), qty("1")),
                (BookSide::Ask, px("101"), qty("3")),
            ],
        });
        book.apply(&BookData::Update {
            levels: vec![
                (BookSide::Bid, px("99"), qty("0")),
                (BookSide::Ask, px("101"), qty("4")),
            ],
        });
        assert_eq!(
            book.snapshot(),
            BookData::Snapshot {
                bids: vec![(px("100"), qty("2"))],
                asks: vec![(px("101"), qty("4")), (px("102"), qty("1"))],
            }
        );
        assert_eq!(book.best_bid(), Some((px("100"), qty("2"))));
        assert_eq!(book.best_ask(), Some((px("101"), qty("4"))));

        let mut copy = Book::default();
        copy.apply(&book.snapshot());
        assert_eq!(copy, book);
    }

    #[test]
    fn book_levels_are_exact() {
        let mut book = Book::default();
        book.apply(&BookData::Snapshot {
            bids: vec![(px("83470.9"), qty("0.1")), (px("83470.8"), qty("0.2"))],
            asks: vec![(px("83471.0"), qty("0.3"))],
        });
        book.apply(&BookData::Update {
            levels: vec![
                (BookSide::Bid, px("83470.90"), qty("0.30000000")),
                (BookSide::Bid, px("83470.8"), qty("0.0")),
                (BookSide::Ask, px("83471.1"), qty("0.1")),
            ],
        });
        assert_eq!(
            book.snapshot(),
            BookData::Snapshot {
                bids: vec![(px("83470.9"), qty("0.3"))],
                asks: vec![(px("83471.0"), qty("0.3")), (px("83471.1"), qty("0.1"))],
            }
        );
    }

    #[test]
    fn prices_of_any_scale_are_one_level() {
        let mut book = Book::default();
        book.apply(&BookData::Update {
            levels: vec![
                (BookSide::Bid, px("100.0"), qty("1")),
                (BookSide::Bid, px("100.00"), qty("2")),
            ],
        });
        assert_eq!(
            book.snapshot(),
            BookData::Snapshot {
                bids: vec![(px("100"), qty("2"))],
                asks: vec![],
            }
        );
    }
}
