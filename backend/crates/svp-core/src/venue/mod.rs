//! Venue adapters: each one turns a venue-specific configuration into a
//! Nautilus data client the node can host.
//!
//! Adding a venue means adding a module here that implements [`Feed`]; the node
//! and the actors only ever see the trait.

pub mod binance;

use std::fmt::Debug;

use nautilus_common::factories::{ClientConfig, DataClientFactory};
use nautilus_model::identifiers::{ClientId, InstrumentId};

/// The pair `LiveNode` needs to register a data client.
#[derive(Debug)]
pub struct DataClientSpec {
    /// Builds the client when the node starts.
    pub factory: Box<dyn DataClientFactory>,
    /// Adapter-specific configuration handed to `factory`.
    pub config: Box<dyn ClientConfig>,
}

/// A source of market data: one data client and the instruments it serves.
pub trait Feed: Debug {
    /// Unique name of the data client within the node.
    ///
    /// Distinct from the venue: several clients can serve one venue (Binance
    /// spot and futures are both `BINANCE`), so subscriptions are routed by
    /// client, not by venue.
    fn client_id(&self) -> ClientId;

    /// Instruments to load and subscribe to on this client.
    fn instrument_ids(&self) -> &[InstrumentId];

    /// Builds the factory and configuration of the data client.
    fn data_client(&self) -> DataClientSpec;
}

/// One instrument on one data client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subscription {
    /// Data client that serves the instrument.
    pub client_id: ClientId,
    /// Instrument to subscribe to.
    pub instrument_id: InstrumentId,
}

/// Flattens the instruments of every feed into subscriptions.
pub fn subscriptions(feeds: &[Box<dyn Feed>]) -> Vec<Subscription> {
    feeds
        .iter()
        .flat_map(|feed| {
            let client_id = feed.client_id();
            feed.instrument_ids()
                .iter()
                .map(move |&instrument_id| Subscription {
                    client_id,
                    instrument_id,
                })
        })
        .collect()
}
