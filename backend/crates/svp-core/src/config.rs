//! Deployment configuration: everything that varies between installs lives
//! here, not in code.
//!
//! Sources, later ones override earlier ones:
//! 1. `<dir>/default.toml` (required, committed),
//! 2. `<dir>/local.toml` (optional, gitignored),
//! 3. environment variables `SVP__SECTION__KEY` (scalars and comma-separated
//!    lists only).
//!
//! `<dir>` is `$SVP_CONFIG_DIR`, default `./config`.

use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::Context;
use config::{Environment, File, FileFormat};
use nautilus_model::identifiers::InstrumentId;
use serde::Deserialize;

/// Environment variable naming the configuration directory.
pub const ENV_CONFIG_DIR: &str = "SVP_CONFIG_DIR";
/// Default configuration directory, relative to the working directory.
pub const DEFAULT_CONFIG_DIR: &str = "config";
const ENV_PREFIX: &str = "SVP";
const ENV_SEPARATOR: &str = "__";

/// Root of the configuration file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Web server settings.
    #[serde(default)]
    pub server: ServerConfig,
    /// Bar aggregation settings.
    pub bars: BarsConfig,
    /// Data clients, one per entry.
    #[serde(default, rename = "venue")]
    pub venues: Vec<VenueConfig>,
    /// Instruments to subscribe to.
    #[serde(default, rename = "instrument")]
    pub instruments: Vec<InstrumentConfig>,
    /// Canonical asset groups for the UI.
    #[serde(default, rename = "asset")]
    pub assets: Vec<AssetConfig>,
}

/// `[server]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ServerConfig {
    /// Socket address the HTTP/WebSocket server listens on.
    pub bind: SocketAddr,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 8080)),
        }
    }
}

/// `[bars]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarsConfig {
    /// Timeframes built from the trade stream, e.g. `["1m", "5m"]`.
    pub timeframes: Vec<Timeframe>,
}

/// `[[venue]]`: one Nautilus data client.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VenueConfig {
    /// svp's name for this client; referenced by `[[instrument]].venue`.
    pub id: String,
    /// Which Nautilus adapter serves it.
    pub adapter: Adapter,
    /// Whether a client is created for it.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Adapter-specific product type (e.g. `usdm` for Binance).
    #[serde(default)]
    pub product_type: Option<String>,
    /// Adapter-specific settings, interpreted by the adapter.
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

fn default_true() -> bool {
    true
}

/// Nautilus adapters svp knows how to configure. Tier 2 venues are listed so
/// config for them parses; the node reports which are not implemented yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Adapter {
    /// Binance (spot, USD-M and COIN-M futures via `product_type`).
    Binance,
    /// Bybit.
    Bybit,
    /// OKX.
    Okx,
    /// `BitMEX`.
    Bitmex,
    /// Deribit.
    Deribit,
    /// Kraken (spot or futures via `product_type`).
    Kraken,
    /// Hyperliquid.
    Hyperliquid,
    /// dYdX.
    Dydx,
    /// Coinbase spot.
    Coinbase,
    /// Lighter (Tier 2).
    Lighter,
    /// Derive (Tier 2).
    Derive,
    /// Architect (Tier 2).
    Ax,
}

/// `[[instrument]]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstrumentConfig {
    /// The `[[venue]].id` whose client serves this instrument.
    pub venue: String,
    /// Nautilus instrument id, `SYMBOL.VENUE`.
    pub id: String,
}

impl InstrumentConfig {
    /// The parsed Nautilus id.
    ///
    /// # Panics
    ///
    /// Panics if the id is invalid; [`Config::validate`] rejects such configs.
    #[must_use]
    pub fn instrument_id(&self) -> InstrumentId {
        InstrumentId::from_str(&self.id).expect("validated instrument id")
    }
}

/// `[[asset]]`: a canonical asset across venues.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetConfig {
    /// Canonical id, e.g. `btc-perp`.
    pub id: String,
    /// Member `[[instrument]].id`s.
    pub instruments: Vec<String>,
}

/// Unit of a [`Timeframe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeUnit {
    /// Seconds.
    Second,
    /// Minutes.
    Minute,
    /// Hours.
    Hour,
    /// Days.
    Day,
}

impl TimeUnit {
    fn suffix(self) -> char {
        match self {
            Self::Second => 's',
            Self::Minute => 'm',
            Self::Hour => 'h',
            Self::Day => 'd',
        }
    }
}

/// A time bar interval such as `1m` or `4h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct Timeframe {
    /// Number of units, at least 1.
    pub step: u32,
    /// The unit.
    pub unit: TimeUnit,
}

impl FromStr for Timeframe {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || ConfigError::InvalidTimeframe(s.to_string());
        let (digits, suffix) = s.split_at(s.len().checked_sub(1).ok_or_else(invalid)?);
        let unit = match suffix {
            "s" => TimeUnit::Second,
            "m" => TimeUnit::Minute,
            "h" => TimeUnit::Hour,
            "d" => TimeUnit::Day,
            _ => return Err(invalid()),
        };
        let step: u32 = digits.parse().map_err(|_| invalid())?;
        if step == 0 {
            return Err(invalid());
        }
        Ok(Self { step, unit })
    }
}

impl TryFrom<String> for Timeframe {
    type Error = ConfigError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl fmt::Display for Timeframe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.step, self.unit.suffix())
    }
}

/// A semantic error in an otherwise well-formed configuration.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    /// `[bars].timeframes` is empty.
    #[error("[bars].timeframes must list at least one timeframe")]
    NoTimeframes,
    /// A timeframe is not `<n><s|m|h|d>` with n ≥ 1.
    #[error("invalid timeframe {0:?}: expected <n><s|m|h|d> with n >= 1, e.g. \"5m\"")]
    InvalidTimeframe(String),
    /// A timeframe appears twice.
    #[error("duplicate timeframe {0}")]
    DuplicateTimeframe(String),
    /// Two `[[venue]]` entries share an id.
    #[error("duplicate [[venue]] id {0:?}")]
    DuplicateVenue(String),
    /// Two `[[instrument]]` entries share an id.
    #[error("duplicate [[instrument]] id {0:?}")]
    DuplicateInstrument(String),
    /// An instrument id is not a Nautilus `SYMBOL.VENUE` id.
    #[error("[[instrument]] id {id:?} is not a valid Nautilus instrument id: {reason}")]
    InvalidInstrumentId {
        /// The offending id.
        id: String,
        /// Why Nautilus rejected it.
        reason: String,
    },
    /// An instrument references a venue that is not configured.
    #[error("[[instrument]] {instrument:?} references unknown venue {venue:?}")]
    UnknownVenue {
        /// The instrument id.
        instrument: String,
        /// The missing venue id.
        venue: String,
    },
    /// Two `[[asset]]` entries share an id.
    #[error("duplicate [[asset]] id {0:?}")]
    DuplicateAsset(String),
    /// An asset lists no instruments.
    #[error("[[asset]] {0:?} lists no instruments")]
    EmptyAsset(String),
    /// An asset references an instrument that is not configured.
    #[error("[[asset]] {asset:?} references unknown instrument {instrument:?}")]
    UnknownInstrument {
        /// The asset id.
        asset: String,
        /// The missing instrument id.
        instrument: String,
    },
    /// Nothing to subscribe to.
    #[error("no [[instrument]] is bound to an enabled [[venue]]")]
    NoActiveInstruments,
}

impl Config {
    /// Loads and validates the configuration from `$SVP_CONFIG_DIR` (default
    /// `./config`) and the environment.
    ///
    /// # Errors
    ///
    /// Returns an error describing the first file, parse or validation problem.
    pub fn load() -> anyhow::Result<Self> {
        let dir = std::env::var_os(ENV_CONFIG_DIR)
            .map_or_else(|| PathBuf::from(DEFAULT_CONFIG_DIR), PathBuf::from);
        Self::load_from(&dir)
    }

    /// Loads and validates the configuration from `dir` and the environment.
    ///
    /// # Errors
    ///
    /// Returns an error describing the first file, parse or validation problem.
    pub fn load_from(dir: &Path) -> anyhow::Result<Self> {
        Self::load_with(dir, Self::environment())
    }

    /// The `SVP__SECTION__KEY` environment source, reading the process env.
    fn environment() -> Environment {
        Environment::with_prefix(ENV_PREFIX)
            .prefix_separator(ENV_SEPARATOR)
            .separator(ENV_SEPARATOR)
            .try_parsing(true)
            .list_separator(",")
            .with_list_parse_key("bars.timeframes")
    }

    fn load_with(dir: &Path, env: Environment) -> anyhow::Result<Self> {
        let default = dir.join("default.toml");
        let local = dir.join("local.toml");
        let config = config::Config::builder()
            .add_source(
                File::from(default.as_path())
                    .format(FileFormat::Toml)
                    .required(true),
            )
            .add_source(
                File::from(local.as_path())
                    .format(FileFormat::Toml)
                    .required(false),
            )
            .add_source(env)
            .build()
            .with_context(|| format!("loading configuration from {}", dir.display()))?;
        let config: Self = config
            .try_deserialize()
            .with_context(|| format!("parsing configuration from {}", dir.display()))?;
        config
            .validate()
            .with_context(|| format!("validating configuration from {}", dir.display()))?;
        Ok(config)
    }

    /// Parses and validates a TOML document (tests and tooling).
    ///
    /// # Errors
    ///
    /// Returns an error describing the first parse or validation problem.
    pub fn from_toml(toml: &str) -> anyhow::Result<Self> {
        let config: Self = config::Config::builder()
            .add_source(File::from_str(toml, FileFormat::Toml))
            .build()?
            .try_deserialize()?;
        config.validate()?;
        Ok(config)
    }

    /// Checks cross-references and uniqueness.
    ///
    /// # Errors
    ///
    /// Returns the first [`ConfigError`] found.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.bars.timeframes.is_empty() {
            return Err(ConfigError::NoTimeframes);
        }
        let mut seen = HashSet::new();
        for tf in &self.bars.timeframes {
            if !seen.insert(tf) {
                return Err(ConfigError::DuplicateTimeframe(tf.to_string()));
            }
        }

        let mut venue_ids = HashSet::new();
        for venue in &self.venues {
            if !venue_ids.insert(venue.id.as_str()) {
                return Err(ConfigError::DuplicateVenue(venue.id.clone()));
            }
        }

        let mut instrument_ids = HashSet::new();
        for instrument in &self.instruments {
            if !instrument_ids.insert(instrument.id.as_str()) {
                return Err(ConfigError::DuplicateInstrument(instrument.id.clone()));
            }
            InstrumentId::from_str(&instrument.id).map_err(|e| {
                ConfigError::InvalidInstrumentId {
                    id: instrument.id.clone(),
                    reason: e.to_string(),
                }
            })?;
            if !venue_ids.contains(instrument.venue.as_str()) {
                return Err(ConfigError::UnknownVenue {
                    instrument: instrument.id.clone(),
                    venue: instrument.venue.clone(),
                });
            }
        }

        let mut asset_ids = HashSet::new();
        for asset in &self.assets {
            if !asset_ids.insert(asset.id.as_str()) {
                return Err(ConfigError::DuplicateAsset(asset.id.clone()));
            }
            if asset.instruments.is_empty() {
                return Err(ConfigError::EmptyAsset(asset.id.clone()));
            }
            for id in &asset.instruments {
                if !instrument_ids.contains(id.as_str()) {
                    return Err(ConfigError::UnknownInstrument {
                        asset: asset.id.clone(),
                        instrument: id.clone(),
                    });
                }
            }
        }

        if self.active_instruments().next().is_none() {
            return Err(ConfigError::NoActiveInstruments);
        }
        Ok(())
    }

    /// Venues with `enabled = true`.
    pub fn enabled_venues(&self) -> impl Iterator<Item = &VenueConfig> {
        self.venues.iter().filter(|v| v.enabled)
    }

    /// The venue entry with the given id.
    #[must_use]
    pub fn venue(&self, id: &str) -> Option<&VenueConfig> {
        self.venues.iter().find(|v| v.id == id)
    }

    /// Instruments bound to an enabled venue.
    pub fn active_instruments(&self) -> impl Iterator<Item = &InstrumentConfig> {
        self.instruments
            .iter()
            .filter(|i| self.venue(&i.venue).is_some_and(|v| v.enabled))
    }

    /// Instruments bound to the given venue, enabled or not.
    pub fn instruments_for(&self, venue_id: &str) -> impl Iterator<Item = &InstrumentConfig> {
        self.instruments.iter().filter(move |i| i.venue == venue_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
        [bars]
        timeframes = ["1m", "1h"]

        [[venue]]
        id = "binance-usdm"
        adapter = "binance"
        product_type = "usdm"

        [[instrument]]
        venue = "binance-usdm"
        id = "BTCUSDT-PERP.BINANCE"

        [[asset]]
        id = "btc-perp"
        instruments = ["BTCUSDT-PERP.BINANCE"]
    "#;

    fn validation_error(toml: &str) -> ConfigError {
        let err = Config::from_toml(toml).unwrap_err();
        err.downcast::<ConfigError>()
            .unwrap_or_else(|e| panic!("expected a ConfigError, got: {e:#}"))
    }

    #[test]
    fn minimal_config_parses_with_defaults() {
        let config = Config::from_toml(MINIMAL).unwrap();
        assert_eq!(config.server.bind, ServerConfig::default().bind);
        assert_eq!(config.bars.timeframes.len(), 2);
        assert_eq!(
            config.bars.timeframes[1],
            Timeframe {
                step: 1,
                unit: TimeUnit::Hour
            }
        );
        assert!(config.venues[0].enabled);
        assert_eq!(config.venues[0].adapter, Adapter::Binance);
        assert_eq!(config.venues[0].product_type.as_deref(), Some("usdm"));
        assert_eq!(
            config.instruments[0].instrument_id(),
            InstrumentId::from("BTCUSDT-PERP.BINANCE")
        );
        assert_eq!(config.active_instruments().count(), 1);
    }

    #[test]
    fn committed_default_config_is_valid() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
        let config = Config::load_from(&dir).unwrap();
        assert!(config.enabled_venues().count() >= 1);
        assert_eq!(config.bars.timeframes.len(), 4);
    }

    #[test]
    fn local_toml_and_env_override_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("default.toml"), MINIMAL).unwrap();
        std::fs::write(
            dir.path().join("local.toml"),
            "[server]\nbind = \"0.0.0.0:9000\"\n",
        )
        .unwrap();
        let config = Config::load_from(dir.path()).unwrap();
        assert_eq!(config.server.bind, "0.0.0.0:9000".parse().unwrap());

        // Injected instead of `set_var`: the process environment is global
        // and `unsafe_code` is forbidden in this workspace.
        let fake_env = std::collections::HashMap::from([(
            "SVP__BARS__TIMEFRAMES".to_string(),
            "5m,15m".to_string(),
        )]);
        let config =
            Config::load_with(dir.path(), Config::environment().source(Some(fake_env))).unwrap();
        assert_eq!(
            config.bars.timeframes,
            vec!["5m".parse().unwrap(), "15m".parse().unwrap()]
        );
    }

    #[test]
    fn missing_default_file_is_a_readable_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = Config::load_from(dir.path()).unwrap_err();
        assert!(format!("{err:#}").contains("default.toml"), "{err:#}");
    }

    #[test]
    fn unknown_field_is_rejected() {
        let err = Config::from_toml(&MINIMAL.replace("[bars]", "[bars]\nbogus = 1")).unwrap_err();
        assert!(format!("{err:#}").contains("bogus"), "{err:#}");
    }

    #[test]
    fn unknown_adapter_is_rejected() {
        let err = Config::from_toml(&MINIMAL.replace("\"binance\"", "\"ftx\"")).unwrap_err();
        assert!(format!("{err:#}").contains("ftx"), "{err:#}");
    }

    #[test]
    fn timeframes_parse_and_display() {
        for s in ["1s", "1m", "15m", "4h", "1d"] {
            assert_eq!(s.parse::<Timeframe>().unwrap().to_string(), s);
        }
        for s in ["", "m", "0m", "1w", "1M", "-1m", "1.5m"] {
            assert_eq!(
                s.parse::<Timeframe>(),
                Err(ConfigError::InvalidTimeframe(s.into()))
            );
        }
    }

    #[test]
    fn validation_errors() {
        assert_eq!(
            validation_error(&MINIMAL.replace("[\"1m\", \"1h\"]", "[]")),
            ConfigError::NoTimeframes
        );
        assert_eq!(
            validation_error(&MINIMAL.replace("[\"1m\", \"1h\"]", "[\"1m\", \"1m\"]")),
            ConfigError::DuplicateTimeframe("1m".into())
        );
        assert_eq!(
            validation_error(&MINIMAL.replace("venue = \"binance-usdm\"", "venue = \"bybit\"")),
            ConfigError::UnknownVenue {
                instrument: "BTCUSDT-PERP.BINANCE".into(),
                venue: "bybit".into()
            }
        );
        assert!(matches!(
            validation_error(&MINIMAL.replace("id = \"BTCUSDT-PERP.BINANCE\"", "id = \"BTCUSDT\"")),
            ConfigError::InvalidInstrumentId { .. }
        ));
        assert_eq!(
            validation_error(&MINIMAL.replace(
                "instruments = [\"BTCUSDT-PERP.BINANCE\"]",
                "instruments = [\"ETHUSDT-PERP.BINANCE\"]"
            )),
            ConfigError::UnknownInstrument {
                asset: "btc-perp".into(),
                instrument: "ETHUSDT-PERP.BINANCE".into()
            }
        );
        assert_eq!(
            validation_error(&MINIMAL.replace(
                "instruments = [\"BTCUSDT-PERP.BINANCE\"]",
                "instruments = []"
            )),
            ConfigError::EmptyAsset("btc-perp".into())
        );
        assert_eq!(
            validation_error(&MINIMAL.replace("product_type = \"usdm\"", "enabled = false")),
            ConfigError::NoActiveInstruments
        );
        let duplicated =
            format!("{MINIMAL}\n[[venue]]\nid = \"binance-usdm\"\nadapter = \"binance\"\n");
        assert_eq!(
            validation_error(&duplicated),
            ConfigError::DuplicateVenue("binance-usdm".into())
        );
        let duplicated = format!(
            "{MINIMAL}\n[[instrument]]\nvenue = \"binance-usdm\"\nid = \"BTCUSDT-PERP.BINANCE\"\n"
        );
        assert_eq!(
            validation_error(&duplicated),
            ConfigError::DuplicateInstrument("BTCUSDT-PERP.BINANCE".into())
        );
        let duplicated = format!(
            "{MINIMAL}\n[[asset]]\nid = \"btc-perp\"\ninstruments = [\"BTCUSDT-PERP.BINANCE\"]\n"
        );
        assert_eq!(
            validation_error(&duplicated),
            ConfigError::DuplicateAsset("btc-perp".into())
        );
    }
}
