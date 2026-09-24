//! Where unified data goes once it leaves the aggregator: the server hands
//! [`crate::aggregator::node::build`] its sinks, and the aggregator doesn't know what
//! they do with it.

use std::fmt::Debug;

use svp_wire::{BookData, Message, Side};

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
