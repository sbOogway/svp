//! Venues, markets and coins, and how they map onto Nautilus data clients.
//!
//! A [`FeedsBuilder`] takes venues, markets and coins separately and combines
//! them: one [`Feed`] (data client) per venue and market, subscribed to the
//! USD instrument of every coin in that market. Adding a venue means adding a
//! [`Venue`] variant and a module that spells a coin's USD instrument in each
//! market and configures the adapter.

mod binance;
mod bybit;
mod hyperliquid;
mod kraken;
mod okx;

use std::{fmt, str::FromStr};

use nautilus_common::factories::{ClientConfig, DataClientFactory};
use nautilus_model::identifiers::{self, ClientId, InstrumentId, Symbol};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Venue {
    Binance,
    Bybit,
    Okx,
    Kraken,
    Hyperliquid,
}

impl Venue {
    const fn name(self) -> &'static str {
        match self {
            Self::Binance => "BINANCE",
            Self::Bybit => "BYBIT",
            Self::Okx => "OKX",
            Self::Kraken => "KRAKEN",
            Self::Hyperliquid => "HYPERLIQUID",
        }
    }

    fn instrument_id(self, market: Market, coin: Coin) -> Option<InstrumentId> {
        let base = coin.code();
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Market {
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

/// A supported coin. Every venue quotes it against its own USD currency:
/// USDT on Binance, Bybit and OKX, USD on Kraken and Hyperliquid. Each one is
/// listed in both markets on every venue (Hyperliquid has no spot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Coin {
    Btc,
    Eth,
    Sol,
    Xrp,
    Doge,
    Bnb,
    Ada,
    Avax,
    Link,
    Ltc,
    Dot,
    Trx,
    Sui,
    Bch,
}

impl Coin {
    pub const ALL: [Self; 14] = [
        Self::Btc,
        Self::Eth,
        Self::Sol,
        Self::Xrp,
        Self::Doge,
        Self::Bnb,
        Self::Ada,
        Self::Avax,
        Self::Link,
        Self::Ltc,
        Self::Dot,
        Self::Trx,
        Self::Sui,
        Self::Bch,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Btc => "BTC",
            Self::Eth => "ETH",
            Self::Sol => "SOL",
            Self::Xrp => "XRP",
            Self::Doge => "DOGE",
            Self::Bnb => "BNB",
            Self::Ada => "ADA",
            Self::Avax => "AVAX",
            Self::Link => "LINK",
            Self::Ltc => "LTC",
            Self::Dot => "DOT",
            Self::Trx => "TRX",
            Self::Sui => "SUI",
            Self::Bch => "BCH",
        }
    }
}

impl fmt::Display for Coin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.code().to_ascii_lowercase())
    }
}

impl FromStr for Coin {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        Self::ALL
            .into_iter()
            .find(|coin| coin.code().eq_ignore_ascii_case(s))
            .ok_or_else(|| anyhow::anyhow!("unsupported coin {s:?}"))
    }
}

#[derive(Debug)]
pub struct DataClientSpec {
    pub factory: Box<dyn DataClientFactory>,
    pub config: Box<dyn ClientConfig>,
}

#[derive(Debug, Clone)]
pub struct Feed {
    venue: Venue,
    market: Market,
    instrument_ids: Vec<InstrumentId>,
}

impl Feed {
    /// Distinct from the venue: several clients can serve one venue (Binance
    /// spot and futures are both `BINANCE`), so subscriptions are routed by
    /// client, not by venue.
    pub fn client_id(&self) -> ClientId {
        ClientId::from(format!("{}-{}", self.venue, self.market).as_str())
    }

    pub fn venue(&self) -> Venue {
        self.venue
    }

    pub fn market(&self) -> Market {
        self.market
    }

    pub fn instrument_ids(&self) -> &[InstrumentId] {
        &self.instrument_ids
    }

    pub fn data_client(&self) -> DataClientSpec {
        self.venue.data_client(self.market, &self.instrument_ids)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subscription {
    pub client_id: ClientId,
    pub instrument_id: InstrumentId,
}

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

/// Combines venues, markets and coins into feeds.
///
/// Every venue is paired with every market and every coin. A market the
/// venue does not have (e.g. Hyperliquid spot) is logged and skipped, so
/// adding a venue never breaks the others. Adding the same venue, market or
/// coin twice has no effect.
#[derive(Debug, Default)]
pub struct FeedsBuilder {
    venues: Vec<Venue>,
    markets: Vec<Market>,
    coins: Vec<Coin>,
}

impl FeedsBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn add_venue(mut self, venue: Venue) -> Self {
        push_unique(&mut self.venues, venue);
        self
    }

    #[must_use]
    pub fn add_market(mut self, market: Market) -> Self {
        push_unique(&mut self.markets, market);
        self
    }

    #[must_use]
    pub fn add_instrument(mut self, coin: Coin) -> Self {
        push_unique(&mut self.coins, coin);
        self
    }

    /// # Errors
    ///
    /// Returns an error naming every missing input (no venue, market or coin),
    /// or if no venue has any of the added markets.
    pub fn build(self) -> anyhow::Result<Vec<Feed>> {
        let Self {
            venues,
            markets,
            coins,
        } = self;

        let errors: Vec<_> = [
            ("venue", venues.is_empty()),
            ("market", markets.is_empty()),
            ("instrument", coins.is_empty()),
        ]
        .into_iter()
        .filter(|&(_, empty)| empty)
        .map(|(what, _)| format!("no {what} added"))
        .collect();
        anyhow::ensure!(
            errors.is_empty(),
            "invalid feeds:\n  {}",
            errors.join("\n  ")
        );

        let mut feeds = Vec::new();
        for &venue in &venues {
            for &market in &markets {
                let instrument_ids: Vec<_> = coins
                    .iter()
                    .filter_map(|&coin| {
                        let id = venue.instrument_id(market, coin);
                        if id.is_none() {
                            tracing::info!(%venue, %market, %coin, "not listed, skipped");
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

    fn id(venue: Venue, market: Market, coin: Coin) -> Option<String> {
        venue.instrument_id(market, coin).map(|id| id.to_string())
    }

    #[test]
    fn maps_a_coin_to_each_venue_usd_instrument() {
        use Coin::{Btc, Doge, Eth};
        use Market::{Futures, Spot};
        use Venue::{Binance, Bybit, Hyperliquid, Kraken, Okx};
        let cases = [
            (Binance, Spot, Btc, Some("BTCUSDT.BINANCE")),
            (Binance, Futures, Btc, Some("BTCUSDT-PERP.BINANCE")),
            (Bybit, Spot, Btc, Some("BTCUSDT-SPOT.BYBIT")),
            (Bybit, Futures, Btc, Some("BTCUSDT-LINEAR.BYBIT")),
            (Okx, Spot, Btc, Some("BTC-USDT.OKX")),
            (Okx, Futures, Btc, Some("BTC-USDT-SWAP.OKX")),
            (Kraken, Spot, Btc, Some("BTC/USD.KRAKEN")),
            (Kraken, Futures, Btc, Some("PF_XBTUSD.KRAKEN")),
            (Kraken, Futures, Eth, Some("PF_ETHUSD.KRAKEN")),
            (Kraken, Spot, Doge, Some("DOGE/USD.KRAKEN")),
            (Kraken, Futures, Doge, Some("PF_DOGEUSD.KRAKEN")),
            (Hyperliquid, Spot, Btc, None),
            (Hyperliquid, Futures, Btc, Some("BTC-USD-PERP.HYPERLIQUID")),
        ];
        for (venue, market, coin, expected) in cases {
            assert_eq!(
                id(venue, market, coin).as_deref(),
                expected,
                "{venue} {market} {coin}"
            );
        }
    }

    #[test]
    fn every_coin_is_listed_on_every_venue() {
        for venue in ALL_VENUES {
            for market in [Market::Spot, Market::Futures] {
                for coin in Coin::ALL {
                    let listed = id(venue, market, coin).is_some();
                    let expected = !(venue == Venue::Hyperliquid && market == Market::Spot);
                    assert_eq!(listed, expected, "{venue} {market} {coin}");
                }
            }
        }
    }

    #[test]
    fn combines_venues_markets_and_coins() {
        let feeds = FeedsBuilder::new()
            .add_venue(Venue::Binance)
            .add_venue(Venue::Bybit)
            .add_market(Market::Spot)
            .add_market(Market::Futures)
            .add_instrument(Coin::Btc)
            .add_instrument(Coin::Eth)
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
    fn skips_markets_a_venue_does_not_have() {
        let feeds = FeedsBuilder::new()
            .add_venue(Venue::Hyperliquid)
            .add_market(Market::Spot)
            .add_market(Market::Futures)
            .add_instrument(Coin::Btc)
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
            .add_instrument(Coin::Btc)
            .build()
            .unwrap_err()
            .to_string();
        assert!(err.contains("no venue lists"), "{err}");
    }

    #[test]
    fn reports_every_missing_input() {
        let err = FeedsBuilder::new().build().unwrap_err().to_string();
        for what in ["venue", "market", "instrument"] {
            assert!(err.contains(&format!("no {what} added")), "{err}");
        }
    }

    #[test]
    fn ignores_duplicates() {
        let feeds = FeedsBuilder::new()
            .add_venue(Venue::Okx)
            .add_venue(Venue::Okx)
            .add_market(Market::Futures)
            .add_market(Market::Futures)
            .add_instrument(Coin::Btc)
            .add_instrument(Coin::Btc)
            .build()
            .unwrap();
        assert_eq!(subscriptions(&feeds).len(), 1);
    }

    #[test]
    fn parses_names_case_insensitively() {
        for venue in ALL_VENUES {
            assert_eq!(venue.to_string().parse::<Venue>().unwrap(), venue);
        }
        for coin in Coin::ALL {
            assert_eq!(coin.to_string().parse::<Coin>().unwrap(), coin);
        }
        assert_eq!("BTC".parse::<Coin>().unwrap(), Coin::Btc);
        assert!("pepe".parse::<Coin>().is_err());
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
