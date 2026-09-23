//! Venues, markets and assets, and how they map onto Nautilus data clients.
//!
//! A [`FeedsBuilder`] takes venues, markets and assets separately and combines
//! them: one [`Feed`] (data client) per venue and market, subscribed to the
//! USD instrument of every asset that venue lists in that market. Adding a
//! venue means adding a [`Venue`] variant and a module that spells an asset's
//! USD instrument in each market and configures the adapter.

mod binance;
mod bybit;
mod hyperliquid;
mod kraken;
mod okx;

use std::{fmt, str::FromStr};

use nautilus_common::factories::{ClientConfig, DataClientFactory};
use nautilus_model::identifiers::{self, ClientId, InstrumentId, Symbol};

/// A supported exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Venue {
    /// Binance.
    Binance,
    /// Bybit.
    Bybit,
    /// OKX.
    Okx,
    /// Kraken.
    Kraken,
    /// Hyperliquid.
    Hyperliquid,
}

impl Venue {
    /// The Nautilus venue name, suffix of every instrument id on it.
    const fn name(self) -> &'static str {
        match self {
            Self::Binance => "BINANCE",
            Self::Bybit => "BYBIT",
            Self::Okx => "OKX",
            Self::Kraken => "KRAKEN",
            Self::Hyperliquid => "HYPERLIQUID",
        }
    }

    /// The id of `asset`'s USD instrument in `market`, whatever the venue
    /// quotes it in (USD, USDT…), or `None` if the venue has no such market.
    fn instrument_id(self, market: Market, asset: &Asset) -> Option<InstrumentId> {
        let base = asset.as_str();
        let symbol = match self {
            Self::Binance => Some(binance::symbol(market, base)),
            Self::Bybit => Some(bybit::symbol(market, base)),
            Self::Okx => Some(okx::symbol(market, base)),
            Self::Kraken => Some(kraken::symbol(market, base)),
            Self::Hyperliquid => hyperliquid::symbol(market, base),
        }?;
        Some(InstrumentId::new(
            Symbol::new(symbol),
            identifiers::Venue::new(self.name()),
        ))
    }

    fn data_client(self, market: Market, instrument_ids: &[InstrumentId]) -> DataClientSpec {
        match self {
            Self::Binance => binance::data_client(market, instrument_ids),
            Self::Bybit => bybit::data_client(market),
            Self::Okx => okx::data_client(market),
            Self::Kraken => kraken::data_client(market),
            Self::Hyperliquid => hyperliquid::data_client(),
        }
    }
}

impl fmt::Display for Venue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Venue {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "binance" => Ok(Self::Binance),
            "bybit" => Ok(Self::Bybit),
            "okx" => Ok(Self::Okx),
            "kraken" => Ok(Self::Kraken),
            "hyperliquid" => Ok(Self::Hyperliquid),
            _ => anyhow::bail!("unknown venue {s:?}"),
        }
    }
}

/// A kind of instrument, named the same on every venue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Market {
    /// Spot.
    Spot,
    /// Linear perpetual swaps, margined in the quote currency.
    Futures,
}

impl fmt::Display for Market {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Spot => "SPOT",
            Self::Futures => "FUTURES",
        })
    }
}

impl FromStr for Market {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "spot" => Ok(Self::Spot),
            "futures" => Ok(Self::Futures),
            _ => anyhow::bail!("unknown market {s:?}"),
        }
    }
}

/// A base asset, e.g. `btc`. Every venue quotes it against its own USD
/// currency: USDT on Binance, Bybit and OKX, USD on Kraken and Hyperliquid.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Asset(String);

impl Asset {
    /// The asset code, upper case.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_ascii_lowercase())
    }
}

impl FromStr for Asset {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric()),
            "invalid asset {s:?}, expected a code such as btc"
        );
        Ok(Self(s.to_ascii_uppercase()))
    }
}

/// The pair `LiveNode` needs to register a data client.
#[derive(Debug)]
pub struct DataClientSpec {
    /// Builds the client when the node starts.
    pub factory: Box<dyn DataClientFactory>,
    /// Adapter-specific configuration handed to `factory`.
    pub config: Box<dyn ClientConfig>,
}

/// One data client: a market on a venue and the instruments it serves.
#[derive(Debug, Clone)]
pub struct Feed {
    venue: Venue,
    market: Market,
    instrument_ids: Vec<InstrumentId>,
}

impl Feed {
    /// Unique name of the data client within the node, e.g. `BINANCE-FUTURES`.
    ///
    /// Distinct from the venue: several clients can serve one venue (Binance
    /// spot and futures are both `BINANCE`), so subscriptions are routed by
    /// client, not by venue.
    pub fn client_id(&self) -> ClientId {
        ClientId::from(format!("{}-{}", self.venue, self.market).as_str())
    }

    /// Venue of the client.
    pub fn venue(&self) -> Venue {
        self.venue
    }

    /// Market of the client.
    pub fn market(&self) -> Market {
        self.market
    }

    /// Instruments to load and subscribe to on this client.
    pub fn instrument_ids(&self) -> &[InstrumentId] {
        &self.instrument_ids
    }

    /// Builds the factory and configuration of the data client.
    pub fn data_client(&self) -> DataClientSpec {
        self.venue.data_client(self.market, &self.instrument_ids)
    }
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
pub fn subscriptions(feeds: &[Feed]) -> Vec<Subscription> {
    feeds
        .iter()
        .flat_map(|feed| {
            let client_id = feed.client_id();
            feed.instrument_ids
                .iter()
                .map(move |&instrument_id| Subscription {
                    client_id,
                    instrument_id,
                })
        })
        .collect()
}

/// Combines venues, markets and assets into feeds.
///
/// Every venue is paired with every market and every asset. A market the
/// venue does not have (e.g. Hyperliquid spot) is logged and skipped, so
/// adding a venue never breaks the others. Adding the same venue, market or
/// asset twice has no effect.
#[derive(Debug, Default)]
pub struct FeedsBuilder {
    venues: Vec<Venue>,
    markets: Vec<Market>,
    assets: Vec<Asset>,
    errors: Vec<String>,
}

impl FeedsBuilder {
    /// Creates an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a venue.
    #[must_use]
    pub fn add_venue(mut self, venue: Venue) -> Self {
        push_unique(&mut self.venues, venue);
        self
    }

    /// Adds a market.
    #[must_use]
    pub fn add_market(mut self, market: Market) -> Self {
        push_unique(&mut self.markets, market);
        self
    }

    /// Adds an asset, e.g. `btc`: its USD instrument on every venue. A
    /// malformed asset is reported by [`build`](Self::build).
    #[must_use]
    pub fn add_instrument(mut self, asset: &str) -> Self {
        match asset.parse() {
            Ok(asset) => push_unique(&mut self.assets, asset),
            Err(e) => self.errors.push(e.to_string()),
        }
        self
    }

    /// Returns one feed per venue and market that lists at least one asset.
    ///
    /// # Errors
    ///
    /// Returns every problem found: a malformed asset, no venue, market or
    /// asset, or no combination listed on any venue.
    pub fn build(self) -> anyhow::Result<Vec<Feed>> {
        let Self {
            venues,
            markets,
            assets,
            mut errors,
        } = self;

        for (what, empty) in [
            ("venue", venues.is_empty()),
            ("market", markets.is_empty()),
            ("instrument", assets.is_empty()),
        ] {
            if empty {
                errors.push(format!("no {what} added"));
            }
        }
        anyhow::ensure!(
            errors.is_empty(),
            "invalid feeds:\n  {}",
            errors.join("\n  ")
        );

        let mut feeds = Vec::new();
        for &venue in &venues {
            for &market in &markets {
                let instrument_ids: Vec<_> = assets
                    .iter()
                    .filter_map(|asset| {
                        let id = venue.instrument_id(market, asset);
                        if id.is_none() {
                            tracing::info!(%venue, %market, %asset, "not listed, skipped");
                        }
                        id
                    })
                    .collect();
                if !instrument_ids.is_empty() {
                    feeds.push(Feed {
                        venue,
                        market,
                        instrument_ids,
                    });
                }
            }
        }

        anyhow::ensure!(
            !feeds.is_empty(),
            "no venue lists any of the added instruments in the added markets"
        );
        Ok(feeds)
    }
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_VENUES: [Venue; 5] = [
        Venue::Binance,
        Venue::Bybit,
        Venue::Okx,
        Venue::Kraken,
        Venue::Hyperliquid,
    ];

    fn id(venue: Venue, market: Market, asset: &str) -> Option<String> {
        venue
            .instrument_id(market, &asset.parse().unwrap())
            .map(|id| id.to_string())
    }

    #[test]
    fn maps_an_asset_to_each_venue_usd_instrument() {
        use Market::{Futures, Spot};
        use Venue::{Binance, Bybit, Hyperliquid, Kraken, Okx};
        let cases = [
            (Binance, Spot, "btc", Some("BTCUSDT.BINANCE")),
            (Binance, Futures, "btc", Some("BTCUSDT-PERP.BINANCE")),
            (Bybit, Spot, "btc", Some("BTCUSDT-SPOT.BYBIT")),
            (Bybit, Futures, "btc", Some("BTCUSDT-LINEAR.BYBIT")),
            (Okx, Spot, "btc", Some("BTC-USDT.OKX")),
            (Okx, Futures, "btc", Some("BTC-USDT-SWAP.OKX")),
            (Kraken, Spot, "btc", Some("BTC/USD.KRAKEN")),
            (Kraken, Futures, "btc", Some("PF_XBTUSD.KRAKEN")),
            (Kraken, Futures, "eth", Some("PF_ETHUSD.KRAKEN")),
            (Hyperliquid, Spot, "btc", None),
            (
                Hyperliquid,
                Futures,
                "btc",
                Some("BTC-USD-PERP.HYPERLIQUID"),
            ),
        ];
        for (venue, market, asset, expected) in cases {
            assert_eq!(
                id(venue, market, asset).as_deref(),
                expected,
                "{venue} {market} {asset}"
            );
        }
    }

    #[test]
    fn combines_venues_markets_and_assets() {
        let feeds = FeedsBuilder::new()
            .add_venue(Venue::Binance)
            .add_venue(Venue::Bybit)
            .add_market(Market::Spot)
            .add_market(Market::Futures)
            .add_instrument("btc")
            .add_instrument("eth")
            .build()
            .unwrap();
        let clients: Vec<_> = feeds.iter().map(|f| f.client_id().to_string()).collect();
        assert_eq!(
            clients,
            [
                "BINANCE-SPOT",
                "BINANCE-FUTURES",
                "BYBIT-SPOT",
                "BYBIT-FUTURES"
            ]
        );
        assert!(feeds.iter().all(|f| f.instrument_ids().len() == 2));
        assert_eq!(subscriptions(&feeds).len(), 8);
    }

    #[test]
    fn one_asset_covers_every_venue_perpetual() {
        let mut builder = FeedsBuilder::new().add_market(Market::Futures);
        for venue in ALL_VENUES {
            builder = builder.add_venue(venue);
        }
        let feeds = builder.add_instrument("btc").build().unwrap();
        let ids: Vec<_> = feeds
            .iter()
            .flat_map(|f| f.instrument_ids().iter().map(ToString::to_string))
            .collect();
        assert_eq!(
            ids,
            [
                "BTCUSDT-PERP.BINANCE",
                "BTCUSDT-LINEAR.BYBIT",
                "BTC-USDT-SWAP.OKX",
                "PF_XBTUSD.KRAKEN",
                "BTC-USD-PERP.HYPERLIQUID",
            ]
        );
    }

    #[test]
    fn skips_markets_a_venue_does_not_have() {
        let feeds = FeedsBuilder::new()
            .add_venue(Venue::Hyperliquid)
            .add_market(Market::Spot)
            .add_market(Market::Futures)
            .add_instrument("btc")
            .build()
            .unwrap();
        let clients: Vec<_> = feeds.iter().map(|f| f.client_id().to_string()).collect();
        assert_eq!(clients, ["HYPERLIQUID-FUTURES"]);
    }

    #[test]
    fn fails_when_nothing_is_listed() {
        let err = FeedsBuilder::new()
            .add_venue(Venue::Hyperliquid)
            .add_market(Market::Spot)
            .add_instrument("btc")
            .build()
            .unwrap_err()
            .to_string();
        assert!(err.contains("no venue lists"), "{err}");
    }

    #[test]
    fn reports_every_input_error() {
        let err = FeedsBuilder::new()
            .add_instrument("btc_usdt")
            .build()
            .unwrap_err()
            .to_string();
        assert!(err.contains("invalid asset \"btc_usdt\""), "{err}");
        assert!(err.contains("no venue added"), "{err}");
        assert!(err.contains("no market added"), "{err}");
    }

    #[test]
    fn ignores_duplicates() {
        let feeds = FeedsBuilder::new()
            .add_venue(Venue::Okx)
            .add_venue(Venue::Okx)
            .add_market(Market::Futures)
            .add_market(Market::Futures)
            .add_instrument("btc")
            .add_instrument("BTC")
            .build()
            .unwrap();
        assert_eq!(subscriptions(&feeds).len(), 1);
    }

    #[test]
    fn parses_names_case_insensitively() {
        for venue in ALL_VENUES {
            assert_eq!(venue.to_string().parse::<Venue>().unwrap(), venue);
        }
        assert_eq!("Futures".parse::<Market>().unwrap(), Market::Futures);
        assert!("perp".parse::<Market>().is_err());
    }

    #[test]
    fn client_ids_are_unique_per_venue_and_market() {
        let mut seen = std::collections::HashSet::new();
        for venue in ALL_VENUES {
            for market in [Market::Spot, Market::Futures] {
                let feed = Feed {
                    venue,
                    market,
                    instrument_ids: vec![],
                };
                assert!(seen.insert(feed.client_id()));
            }
        }
    }
}
