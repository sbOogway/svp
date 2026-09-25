//! One [`Message`](super::Message) or [`Request`](super::Request) per frame,
//! in `MessagePack`. Framing is the transport's job.

use bytes::Bytes;
use serde::{Serialize, de::DeserializeOwned};

pub use rmp_serde::decode::Error as DecodeError;

pub fn encode(message: &impl Serialize) -> Bytes {
    // Named: the protocol's internally tagged enums need field names on the wire.
    rmp_serde::to_vec_named(message)
        .expect("every protocol type is representable in MessagePack")
        .into()
}

pub fn decode<T: DeserializeOwned>(frame: &[u8]) -> Result<T, DecodeError> {
    rmp_serde::from_slice(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::Coin;
    use crate::protocol::{
        BookData, BookSide, BookUpdate, Instrument, Market, Message, PROTOCOL_VERSION, Price,
        Quantity, Side, Trade,
    };

    fn px(s: &str) -> Price {
        s.parse().unwrap()
    }

    fn qty(s: &str) -> Quantity {
        s.parse().unwrap()
    }

    #[test]
    fn every_message_round_trips() {
        let book = |data| {
            Message::Book(BookUpdate {
                instrument: "BTC-PERP.SVP".into(),
                ts: 2,
                data,
            })
        };
        let messages = [
            Message::Trade(Trade {
                instrument: "BTC-PERP.SVP".into(),
                ts: 1,
                price: px("83471.5"),
                size: qty("0.25"),
                aggressor: Some(Side::Buy),
                id: "abc-BIN".into(),
            }),
            Message::Trade(Trade {
                instrument: "BTC-PERP.SVP".into(),
                ts: 1,
                price: px("83471.5"),
                size: qty("0.25"),
                aggressor: None,
                id: "abc-BIN".into(),
            }),
            book(BookData::Snapshot {
                bids: vec![(px("83471.0"), qty("1.2"))],
                asks: vec![(px("83472.0"), qty("0.5"))],
            }),
            book(BookData::Update {
                levels: vec![
                    (BookSide::Bid, px("83470.9"), qty("0.4")),
                    (BookSide::Ask, px("83472.0"), qty("0")),
                ],
            }),
            Message::Resync { missed: 7 },
            Message::Welcome {
                session: 1,
                version: PROTOCOL_VERSION,
                instruments: vec![Instrument {
                    id: "BTC-PERP.SVP".into(),
                    coin: Coin::BTC,
                    market: Market::Perp,
                    venues: vec!["BINANCE".into()],
                }],
            },
            Message::Error {
                reason: "unknown".into(),
            },
            Message::Reject {
                reason: "no".into(),
            },
            Message::Goodbye {
                reason: "bye".into(),
            },
        ];
        for message in messages {
            assert_eq!(decode::<Message>(&encode(&message)).unwrap(), message);
        }
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(decode::<Message>(&[0xc1]).is_err());
    }
}
