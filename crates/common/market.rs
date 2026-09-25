//! What markets are made of, known to the server and its clients alike:
//! the [`Coin`]s, and how many decimals their prices and sizes are shown with.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// Declares an enum with `ALL`, `as_str`, `Display` and case-insensitive
/// `FromStr`, all derived from one list. A variant's string is its name, or the
/// literal after `=` (`Binance = "BINANCE"`).
#[macro_export]
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
                    $(Self::$variant => $crate::named_enum!(@string $variant $($string)?)),+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::market::UnknownName;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|v| v.as_str().eq_ignore_ascii_case(s))
                    .ok_or_else(|| {
                        $crate::market::UnknownName(format!("unknown {} {s:?}", stringify!($name)))
                    })
            }
        }
    };
    (@string $variant:ident) => { stringify!($variant) };
    (@string $variant:ident $string:literal) => { $string };
}

/// A name no variant of a [`named_enum!`] answers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownName(pub String);

impl fmt::Display for UnknownName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UnknownName {}

crate::named_enum! {
    /// The coins the server can merge. On the wire, its name (`"BTC"`).
    pub enum Coin {
        BTC, ETH, SOL, XRP, DOGE, BNB, ADA, AVAX, LINK, LTC, DOT, TRX, SUI, BCH,
    }
}

impl Coin {
    /// How many decimals clients show its USD prices with.
    pub fn price_decimals(self) -> u8 {
        match self {
            Self::BTC | Self::ETH | Self::SOL | Self::BNB | Self::LTC | Self::BCH => 2,
            Self::AVAX | Self::LINK | Self::DOT => 3,
            Self::XRP | Self::ADA | Self::SUI => 4,
            Self::DOGE | Self::TRX => 5,
        }
    }

    /// How many decimals clients show its sizes in coins with.
    pub fn size_decimals(self) -> u8 {
        match self {
            Self::BTC => 5,
            Self::ETH => 4,
            Self::BNB | Self::BCH => 3,
            Self::SOL | Self::AVAX | Self::LTC => 2,
            Self::LINK | Self::DOT | Self::SUI => 1,
            Self::XRP | Self::DOGE | Self::ADA | Self::TRX => 0,
        }
    }
}

impl Serialize for Coin {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Coin {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        name.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_coin_is_its_name_in_any_case() {
        for &coin in Coin::ALL {
            assert_eq!(coin.to_string().parse::<Coin>().unwrap(), coin);
        }
        assert_eq!("btc".parse::<Coin>().unwrap(), Coin::BTC);
        assert_eq!(
            "pepe".parse::<Coin>().unwrap_err().to_string(),
            "unknown Coin \"pepe\""
        );
        assert_eq!(serde_json::to_string(&Coin::BTC).unwrap(), "\"BTC\"");
        assert!(serde_json::from_str::<Coin>("\"PEPE\"").is_err());
    }
}
