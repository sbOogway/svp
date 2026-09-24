//! One [`Message`] per frame, in `MessagePack`. Framing is the transport's job.

use bytes::Bytes;
use svp_wire::Message;

pub use rmp_serde::decode::Error as DecodeError;

pub fn encode(message: &Message) -> Bytes {
    // Named: `svp_wire`'s internally tagged enums need field names on the wire.
    rmp_serde::to_vec_named(message)
        .expect("every Message is representable in MessagePack")
        .into()
}

pub fn decode(frame: &[u8]) -> Result<Message, DecodeError> {
    rmp_serde::from_slice(frame)
}

#[cfg(test)]
mod tests {
    use svp_wire::{BookData, BookSide, BookUpdate, Side, Trade};

    use crate::sink::tests::{px, qty};

    use super::*;

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
        ];
        for message in messages {
            assert_eq!(decode(&encode(&message)).unwrap(), message);
        }
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(decode(&[0xc1]).is_err());
    }
}
