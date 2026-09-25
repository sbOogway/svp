//! Where unified data goes once it leaves the aggregator: the server hands
//! [`crate::aggregator::node::build`] its sinks, and the aggregator doesn't know what
//! they do with it.

use std::{collections::HashMap, fmt::Debug};

use svp_common::protocol::{BookData, Instrument, Message, Side};

pub trait Sink: Debug {
    fn send(&mut self, message: &Message);
}

/// Logs every message at debug level, prices and sizes with the decimals
/// their instrument shows.
#[derive(Debug, Default)]
pub struct LogSink {
    decimals: HashMap<String, (u8, u8)>,
}

impl LogSink {
    pub fn new(instruments: &[Instrument]) -> Self {
        Self {
            decimals: instruments
                .iter()
                .map(|i| (i.id.clone(), (i.price_decimals, i.size_decimals)))
                .collect(),
        }
    }

    /// Every decimal the units hold for an instrument it wasn't told of.
    fn decimals(&self, instrument: &str) -> (u8, u8) {
        self.decimals
            .get(instrument)
            .copied()
            .unwrap_or((u8::MAX, u8::MAX))
    }
}

impl Sink for LogSink {
    fn send(&mut self, message: &Message) {
        match message {
            Message::Trade(t) => {
                let (price, size) = self.decimals(&t.instrument);
                log::debug!(
                    "trade {} {} {} @ {} id={}",
                    t.instrument,
                    match t.aggressor {
                        Some(Side::Buy) => "buy",
                        Some(Side::Sell) => "sell",
                        None => "-",
                    },
                    t.size.fixed(size),
                    t.price.fixed(price),
                    t.id
                );
            }
            Message::Book(b) => match &b.data {
                BookData::Snapshot { bids, asks } => log::debug!(
                    "book {} snapshot, {} bids, {} asks",
                    b.instrument,
                    bids.len(),
                    asks.len()
                ),
                BookData::Update { levels } => {
                    let (price_decimals, size_decimals) = self.decimals(&b.instrument);
                    for (side, price, size) in levels {
                        log::debug!(
                            "book {} {side:?} {} {}",
                            b.instrument,
                            price.fixed(price_decimals),
                            size.fixed(size_decimals)
                        );
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
