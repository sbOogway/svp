//! Binance: one data client per market (spot, USD-M or COIN-M futures), all
//! on venue `BINANCE`. Public streams only, no API keys.

use nautilus_binance::{
    common::enums::{BinanceEnvironment, BinanceProductType},
    config::{BinanceDataClientConfig, BinanceInstrumentProviderConfig, BinanceSpotMarketDataMode},
    factories::BinanceDataClientFactory,
};
use nautilus_model::identifiers::{ClientId, InstrumentId};

use super::{DataClientSpec, Feed};

/// Binance market served by a data client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinanceMarket {
    /// Spot (`BTCUSDT.BINANCE`).
    Spot,
    /// USD-M futures, linear (`BTCUSDT-PERP.BINANCE`).
    UsdM,
    /// COIN-M futures, inverse (`BTCUSD_PERP-PERP.BINANCE`).
    CoinM,
}

impl BinanceMarket {
    const fn product_type(self) -> BinanceProductType {
        match self {
            Self::Spot => BinanceProductType::Spot,
            Self::UsdM => BinanceProductType::UsdM,
            Self::CoinM => BinanceProductType::CoinM,
        }
    }

    const fn client_name(self) -> &'static str {
        match self {
            Self::Spot => "BINANCE-SPOT",
            Self::UsdM => "BINANCE-USDM",
            Self::CoinM => "BINANCE-COINM",
        }
    }
}

/// A Binance data client for one market.
#[derive(Debug, Clone)]
pub struct BinanceFeed {
    market: BinanceMarket,
    instrument_ids: Vec<InstrumentId>,
}

impl BinanceFeed {
    /// Creates a feed for `instrument_ids` on `market`.
    #[must_use]
    pub fn new(market: BinanceMarket, instrument_ids: Vec<InstrumentId>) -> Self {
        Self {
            market,
            instrument_ids,
        }
    }
}

impl Feed for BinanceFeed {
    fn client_id(&self) -> ClientId {
        ClientId::from(self.market.client_name())
    }

    fn instrument_ids(&self) -> &[InstrumentId] {
        &self.instrument_ids
    }

    fn data_client(&self) -> DataClientSpec {
        // Load only the instruments we subscribe to: `load_all` fetches the
        // whole exchange info (~500 symbols) and warns for every non-trading one.
        let config = BinanceDataClientConfig {
            product_type: self.market.product_type(),
            environment: BinanceEnvironment::Live,
            // SBE (the default) requires Ed25519 keys; ignored off spot.
            spot_market_data_mode: BinanceSpotMarketDataMode::Json,
            instrument_provider: BinanceInstrumentProviderConfig {
                load_all: false,
                load_ids: Some(
                    self.instrument_ids
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                ),
                ..Default::default()
            },
            ..Default::default()
        };
        DataClientSpec {
            factory: Box::new(BinanceDataClientFactory::new()),
            config: Box::new(config),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markets_get_distinct_client_ids() {
        let ids: Vec<_> = [
            BinanceMarket::Spot,
            BinanceMarket::UsdM,
            BinanceMarket::CoinM,
        ]
        .into_iter()
        .map(|m| BinanceFeed::new(m, vec![]).client_id())
        .collect();
        assert_eq!(ids.len(), 3);
        assert_ne!(ids[0], ids[1]);
        assert_ne!(ids[1], ids[2]);
        assert_ne!(ids[0], ids[2]);
    }

    #[test]
    fn config_loads_only_the_feed_instruments() {
        let feed = BinanceFeed::new(
            BinanceMarket::UsdM,
            vec![InstrumentId::from("BTCUSDT-PERP.BINANCE")],
        );
        let spec = feed.data_client();
        let config = spec
            .config
            .as_any()
            .downcast_ref::<BinanceDataClientConfig>()
            .unwrap();
        assert_eq!(config.product_type, BinanceProductType::UsdM);
        assert!(!config.instrument_provider.load_all);
        assert_eq!(
            config.instrument_provider.load_ids.as_deref(),
            Some(&["BTCUSDT-PERP.BINANCE".to_string()][..])
        );
    }
}
