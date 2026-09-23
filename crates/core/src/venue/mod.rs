//! Venues, markets and coins, and how they map onto Nautilus data clients.
//!
//! A [`FeedsBuilder`] takes venues, markets and coins separately and combines
//! them: one [`Feed`] (data client) per venue and market, subscribed to the
//! USD instrument of every coin in that market. Adding a venue means adding a
//! [`Venue`] variant and a module that spells a coin's USD instrument in each
//! market and configures the adapter.

mod binance;
mod bybit;
mod coinbase;
mod hyperliquid;
mod kraken;
mod okx;

use nautilus_common::factories::{ClientConfig, DataClientFactory};
use nautilus_model::identifiers::{self, ClientId, InstrumentId, Symbol};

/// Declares an enum with `ALL`, `as_str`, `Display` and case-insensitive
/// `FromStr`, all derived from one list. A variant's string is its name, or the
/// literal after `=` (`Binance = "BINANCE"`).
macro_rules! named_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident {
        $($(#[$vmeta:meta])* $variant:ident $(= $string:literal)?),+ $(,)?
    }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => named_enum!(@string $variant $($string)?)),+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = anyhow::Error;

            fn from_str(s: &str) -> anyhow::Result<Self> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|v| v.as_str().eq_ignore_ascii_case(s))
                    .ok_or_else(|| anyhow::anyhow!("unknown {} {s:?}", stringify!($name)))
            }
        }
    };
    (@string $variant:ident) => { stringify!($variant) };
    (@string $variant:ident $string:literal) => { $string };
}

/// What each exchange module provides. `symbol` is `None` for a market the
/// exchange doesn't have.
trait Exchange {
    fn symbol(&self, market: Market, coin: Coin) -> Option<Symbol>;
    fn data_client(&self, market: Market, instrument_ids: &[InstrumentId]) -> DataClientSpec;
}

named_enum! {
    // The strings are Nautilus venue names.
    pub enum Venue {
        Binance = "BINANCE",
        Bybit = "BYBIT",
        Okx = "OKX",
        Kraken = "KRAKEN",
        Coinbase = "COINBASE",
        Hyperliquid = "HYPERLIQUID",
    }
}

impl Venue {
    fn exchange(self) -> &'static dyn Exchange {
        match self {
            Self::Binance => &binance::Binance,
            Self::Bybit => &bybit::Bybit,
            Self::Okx => &okx::Okx,
            Self::Kraken => &kraken::Kraken,
            Self::Coinbase => &coinbase::Coinbase,
            Self::Hyperliquid => &hyperliquid::Hyperliquid,
        }
    }

    fn instrument_id(self, market: Market, coin: Coin) -> Option<InstrumentId> {
        let symbol = self.exchange().symbol(market, coin)?;
        Some(InstrumentId::new(
            symbol,
            identifiers::Venue::new(self.as_str()),
        ))
    }
}

named_enum! {
    pub enum Market {
        Spot = "SPOT",
        /// Linear perpetual swaps, margined in the quote currency.
        Futures = "FUTURES",
    }
}

named_enum! {
    /// Every venue quotes a coin against its own USD currency: USDT on Binance,
    /// Bybit and OKX, USD on Kraken, Coinbase and Hyperliquid. Each one is
    /// listed in both markets on every venue, except that Hyperliquid has no
    /// spot and Coinbase has no TRX.
    pub enum Coin {
        BTC, ETH, SOL, XRP, DOGE, BNB, ADA, AVAX, LINK, LTC, DOT, TRX, SUI, BCH,
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
        self.venue
            .exchange()
            .data_client(self.market, &self.instrument_ids)
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

    fn id(venue: Venue, market: Market, coin: Coin) -> Option<String> {
        venue.instrument_id(market, coin).map(|id| id.to_string())
    }

    #[test]
    fn maps_a_coin_to_each_venue_usd_instrument() {
        use Coin::{BTC, DOGE, ETH, TRX};
        use Market::{Futures, Spot};
        use Venue::{Binance, Bybit, Coinbase, Hyperliquid, Kraken, Okx};
        let cases = [
            (Binance, Spot, BTC, Some("BTCUSDT.BINANCE")),
            (Binance, Futures, BTC, Some("BTCUSDT-PERP.BINANCE")),
            (Bybit, Spot, BTC, Some("BTCUSDT-SPOT.BYBIT")),
            (Bybit, Futures, BTC, Some("BTCUSDT-LINEAR.BYBIT")),
            (Okx, Spot, BTC, Some("BTC-USDT.OKX")),
            (Okx, Futures, BTC, Some("BTC-USDT-SWAP.OKX")),
            (Kraken, Spot, BTC, Some("BTC/USD.KRAKEN")),
            (Kraken, Futures, BTC, Some("PF_XBTUSD.KRAKEN")),
            (Kraken, Futures, ETH, Some("PF_ETHUSD.KRAKEN")),
            (Kraken, Spot, DOGE, Some("DOGE/USD.KRAKEN")),
            (Kraken, Futures, DOGE, Some("PF_DOGEUSD.KRAKEN")),
            (Coinbase, Spot, BTC, Some("BTC-USD.COINBASE")),
            (Coinbase, Futures, BTC, Some("BIP-20DEC30-CDE.COINBASE")),
            (Coinbase, Spot, TRX, None),
            (Coinbase, Futures, TRX, None),
            (Hyperliquid, Spot, BTC, None),
            (Hyperliquid, Futures, BTC, Some("BTC-USD-PERP.HYPERLIQUID")),
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
    fn lists_every_coin_except_documented_gaps() {
        for &venue in Venue::ALL {
            for &market in Market::ALL {
                for &coin in Coin::ALL {
                    let listed = id(venue, market, coin).is_some();
                    let expected = match venue {
                        Venue::Hyperliquid => market == Market::Futures,
                        Venue::Coinbase => coin != Coin::TRX,
                        _ => true,
                    };
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
            .add_instrument(Coin::BTC)
            .add_instrument(Coin::ETH)
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
            .add_instrument(Coin::BTC)
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
            .add_instrument(Coin::BTC)
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
            .add_instrument(Coin::BTC)
            .add_instrument(Coin::BTC)
            .build()
            .unwrap();
        assert_eq!(subscriptions(&feeds).len(), 1);
    }

    #[test]
    fn parses_names_case_insensitively() {
        for &venue in Venue::ALL {
            assert_eq!(venue.to_string().parse::<Venue>().unwrap(), venue);
        }
        for &coin in Coin::ALL {
            assert_eq!(coin.to_string().parse::<Coin>().unwrap(), coin);
        }
        assert!("pepe".parse::<Coin>().is_err());
        assert_eq!("btc".parse::<Coin>().unwrap(), Coin::BTC);
        assert_eq!("Futures".parse::<Market>().unwrap(), Market::Futures);
        assert!("perp".parse::<Market>().is_err());
    }

    #[test]
    fn client_ids_are_unique_per_venue_and_market() {
        let mut seen = std::collections::HashSet::new();
        for &venue in Venue::ALL {
            for &market in Market::ALL {
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
