//! Venue adapters: each one turns a venue-specific configuration into a
//! Nautilus data client the node can host.
//!
//! Adding a venue means adding a module here that implements [`Feed`] and a
//! shortcut on [`FeedsBuilder`]; the node and the actors only see the trait.

pub mod binance;
pub mod bybit;
pub mod hyperliquid;
pub mod kraken;
pub mod okx;

use std::{collections::HashSet, fmt::Debug};

use nautilus_common::factories::{ClientConfig, DataClientFactory};
use nautilus_model::identifiers::{ClientId, InstrumentId, Venue};

use self::{
    binance::{BinanceFeed, BinanceMarket},
    bybit::{BybitFeed, BybitMarket},
    hyperliquid::HyperliquidFeed,
    kraken::{KrakenFeed, KrakenMarket},
    okx::{OkxFeed, OkxMarket},
};

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

    /// Venue of every instrument this client serves.
    fn venue(&self) -> Venue;

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

/// Collects the feeds of a node and validates them as a set.
///
/// Instrument ids are given as strings; parse errors are reported by
/// [`build`](Self::build), together with every other problem found.
#[derive(Debug, Default)]
pub struct FeedsBuilder {
    feeds: Vec<Box<dyn Feed>>,
    errors: Vec<String>,
}

impl FeedsBuilder {
    /// Creates an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a Binance client for `market`.
    #[must_use]
    pub fn binance(self, market: BinanceMarket, instrument_ids: &[&str]) -> Self {
        self.parsed(instrument_ids, |ids| BinanceFeed::new(market, ids))
    }

    /// Adds a Bybit client for `market`.
    #[must_use]
    pub fn bybit(self, market: BybitMarket, instrument_ids: &[&str]) -> Self {
        self.parsed(instrument_ids, |ids| BybitFeed::new(market, ids))
    }

    /// Adds an OKX client for `market`.
    #[must_use]
    pub fn okx(self, market: OkxMarket, instrument_ids: &[&str]) -> Self {
        self.parsed(instrument_ids, |ids| OkxFeed::new(market, ids))
    }

    /// Adds a Kraken client for `market`.
    #[must_use]
    pub fn kraken(self, market: KrakenMarket, instrument_ids: &[&str]) -> Self {
        self.parsed(instrument_ids, |ids| KrakenFeed::new(market, ids))
    }

    /// Adds the Hyperliquid client.
    #[must_use]
    pub fn hyperliquid(self, instrument_ids: &[&str]) -> Self {
        self.parsed(instrument_ids, HyperliquidFeed::new)
    }

    /// Adds any feed, for venues without a shortcut.
    #[must_use]
    pub fn feed(mut self, feed: impl Feed + 'static) -> Self {
        self.feeds.push(Box::new(feed));
        self
    }

    fn parsed<F: Feed + 'static>(
        mut self,
        instrument_ids: &[&str],
        new: impl FnOnce(Vec<InstrumentId>) -> F,
    ) -> Self {
        let mut ids = Vec::with_capacity(instrument_ids.len());
        for raw in instrument_ids {
            match raw.parse() {
                Ok(id) => ids.push(id),
                Err(e) => self
                    .errors
                    .push(format!("invalid instrument id {raw:?}: {e}")),
            }
        }
        self.feed(new(ids))
    }

    /// Returns the feeds.
    ///
    /// # Errors
    ///
    /// Returns every problem found: an unparsable instrument id, no feeds, a
    /// feed without instruments, two feeds with the same client id, an
    /// instrument on another venue than its feed, or an instrument added twice.
    pub fn build(self) -> anyhow::Result<Vec<Box<dyn Feed>>> {
        let Self { feeds, mut errors } = self;

        if feeds.is_empty() {
            errors.push("no feeds configured".into());
        }
        let mut client_ids = HashSet::new();
        let mut instrument_ids = HashSet::new();
        for feed in &feeds {
            let client_id = feed.client_id();
            if !client_ids.insert(client_id) {
                errors.push(format!("client {client_id} added twice"));
            }
            if feed.instrument_ids().is_empty() {
                errors.push(format!("client {client_id} has no instruments"));
            }
            for id in feed.instrument_ids() {
                if id.venue != feed.venue() {
                    errors.push(format!(
                        "{id} is not on venue {} of client {client_id}",
                        feed.venue()
                    ));
                }
                if !instrument_ids.insert(*id) {
                    errors.push(format!("{id} added twice"));
                }
            }
        }

        anyhow::ensure!(
            errors.is_empty(),
            "invalid feeds:\n  {}",
            errors.join("\n  ")
        );
        Ok(feeds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_of(builder: FeedsBuilder) -> String {
        builder.build().unwrap_err().to_string()
    }

    #[test]
    fn builds_feeds_across_venues() {
        let feeds = FeedsBuilder::new()
            .binance(BinanceMarket::UsdM, &["BTCUSDT-PERP.BINANCE"])
            .binance(BinanceMarket::Spot, &["BTCUSDT.BINANCE"])
            .hyperliquid(&["BTC-USD-PERP.HYPERLIQUID"])
            .build()
            .unwrap();
        assert_eq!(subscriptions(&feeds).len(), 3);
    }

    #[test]
    fn rejects_empty_set() {
        assert!(error_of(FeedsBuilder::new()).contains("no feeds"));
    }

    #[test]
    fn rejects_unparsable_id() {
        let err = error_of(FeedsBuilder::new().hyperliquid(&["BTC-USD-PERP"]));
        assert!(err.contains("invalid instrument id"), "{err}");
    }

    #[test]
    fn rejects_instrument_on_other_venue() {
        let err = error_of(FeedsBuilder::new().okx(OkxMarket::Swap, &["BTCUSDT-LINEAR.BYBIT"]));
        assert!(err.contains("not on venue OKX"), "{err}");
    }

    #[test]
    fn rejects_duplicate_client_and_instrument() {
        let err = error_of(
            FeedsBuilder::new()
                .kraken(KrakenMarket::Futures, &["PF_XBTUSD.KRAKEN"])
                .kraken(KrakenMarket::Futures, &["PF_XBTUSD.KRAKEN"]),
        );
        assert!(err.contains("client KRAKEN-FUTURES added twice"), "{err}");
        assert!(err.contains("PF_XBTUSD.KRAKEN added twice"), "{err}");
    }

    #[test]
    fn rejects_feed_without_instruments() {
        let err = error_of(FeedsBuilder::new().bybit(BybitMarket::Linear, &[]));
        assert!(err.contains("has no instruments"), "{err}");
    }
}
