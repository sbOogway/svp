//! The data the svp server sends to its clients: trades, book updates and the
//! [`Book`] both sides rebuild from them.
//!
//! Plain serde types with no transport in them; `svp-transport` decides how
//! they are encoded and carried.

use std::{cmp::Ordering, collections::BTreeMap};

use serde::{Deserialize, Serialize};

/// Identifies a bar stream as `venue:symbol:timeframe`, e.g. `BINANCE:BTCUSDT-PERP:1m`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StreamId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Trade(Trade),
    Book(BookUpdate),
    /// The client fell behind and `missed` messages were dropped. A snapshot
    /// of every book follows; trades in the gap are lost.
    Resync {
        missed: u64,
    },
}

/// Prices in USD, sizes in coins, timestamps in UNIX nanoseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub instrument: String,
    pub ts: u64,
    pub price: f64,
    pub size: f64,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookUpdate {
    pub instrument: String,
    pub ts: u64,
    #[serde(flatten)]
    pub data: BookData,
}

/// A whole book, or the levels that changed since the last message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BookData {
    /// Bids best (highest) first, asks best (lowest) first, as `[price, size]`.
    Snapshot {
        bids: Vec<(f64, f64)>,
        asks: Vec<(f64, f64)>,
    },
    /// `[side, price, size]`; size 0 removes the level.
    Update { levels: Vec<(BookSide, f64, f64)> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookSide {
    Bid,
    Ask,
}

/// A book kept from [`BookData`]: a server answers a late client with its
/// snapshot, and the app keeps one to draw.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Book {
    bids: BTreeMap<Px, f64>,
    asks: BTreeMap<Px, f64>,
}

impl Book {
    pub fn apply(&mut self, data: &BookData) {
        match data {
            BookData::Snapshot { bids, asks } => {
                self.bids = bids.iter().map(|&(p, s)| (Px(p), s)).collect();
                self.asks = asks.iter().map(|&(p, s)| (Px(p), s)).collect();
            }
            BookData::Update { levels } => {
                for &(side, price, size) in levels {
                    let book = match side {
                        BookSide::Bid => &mut self.bids,
                        BookSide::Ask => &mut self.asks,
                    };
                    if size == 0.0 {
                        book.remove(&Px(price));
                    } else {
                        book.insert(Px(price), size);
                    }
                }
            }
        }
    }

    pub fn snapshot(&self) -> BookData {
        BookData::Snapshot {
            bids: self.bids.iter().rev().map(|(p, &s)| (p.0, s)).collect(),
            asks: self.asks.iter().map(|(p, &s)| (p.0, s)).collect(),
        }
    }
}

/// A price as a map key: `f64` is not `Ord`.
#[derive(Debug, Clone, Copy)]
struct Px(f64);

impl PartialEq for Px {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Px {}

impl PartialOrd for Px {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Px {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                price: 83471.5,
                size: 0.25,
                aggressor: Some(Side::Sell),
                id: "96728d723fa5443eadbaa76d67f9515a-CBS".into(),
            }),
            r#"{"type":"trade","instrument":"BTC-PERP.SVP","ts":1,"price":83471.5,"size":0.25,"aggressor":"sell","id":"96728d723fa5443eadbaa76d67f9515a-CBS"}"#,
        );
    }

    #[test]
    fn book_on_the_wire() {
        roundtrip(
            &Message::Book(BookUpdate {
                instrument: "BTC-PERP.SVP".into(),
                ts: 1,
                data: BookData::Snapshot {
                    bids: vec![(83471.0, 1.2)],
                    asks: vec![(83472.0, 0.5)],
                },
            }),
            r#"{"type":"book","instrument":"BTC-PERP.SVP","ts":1,"kind":"snapshot","bids":[[83471.0,1.2]],"asks":[[83472.0,0.5]]}"#,
        );
        roundtrip(
            &Message::Book(BookUpdate {
                instrument: "BTC-PERP.SVP".into(),
                ts: 2,
                data: BookData::Update {
                    levels: vec![(BookSide::Bid, 83470.9, 0.4), (BookSide::Ask, 83472.0, 0.0)],
                },
            }),
            r#"{"type":"book","instrument":"BTC-PERP.SVP","ts":2,"kind":"update","levels":[["bid",83470.9,0.4],["ask",83472.0,0.0]]}"#,
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
    fn book_keeps_updates_and_snapshots_them_best_first() {
        let mut book = Book::default();
        book.apply(&BookData::Update {
            levels: vec![
                (BookSide::Bid, 99.0, 1.0),
                (BookSide::Bid, 100.0, 2.0),
                (BookSide::Ask, 102.0, 1.0),
                (BookSide::Ask, 101.0, 3.0),
            ],
        });
        book.apply(&BookData::Update {
            levels: vec![(BookSide::Bid, 99.0, 0.0), (BookSide::Ask, 101.0, 4.0)],
        });
        assert_eq!(
            book.snapshot(),
            BookData::Snapshot {
                bids: vec![(100.0, 2.0)],
                asks: vec![(101.0, 4.0), (102.0, 1.0)],
            }
        );

        let mut copy = Book::default();
        copy.apply(&book.snapshot());
        assert_eq!(copy, book);
    }
}
