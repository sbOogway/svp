//! Exact decimal prices and sizes: a binary float can't hold most decimal
//! prices (`83470.9` isn't one). Both travel as decimal strings, in every
//! format.

use std::{
    fmt,
    ops::{Add, AddAssign, Sub, SubAssign},
    str::FromStr,
};

pub use rust_decimal::{Decimal, Error as DecimalError};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

macro_rules! decimal_newtype {
    ($name:ident, $what:literal) => {
        /// Equal values are equal whatever their scale: `100.0 == 100.00`.
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Decimal);

        impl $name {
            pub const fn new(value: Decimal) -> Self {
                Self(value)
            }

            pub const fn as_decimal(self) -> Decimal {
                self.0
            }
        }

        impl From<Decimal> for $name {
            fn from(value: Decimal) -> Self {
                Self(value)
            }
        }

        /// Fails rather than rounds a value with more digits than a
        /// [`Decimal`] holds.
        impl FromStr for $name {
            type Err = DecimalError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Decimal::from_str_exact(s).map(Self)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        // By hand: a dependency turning on one of `rust_decimal`'s float
        // features would change how `Decimal` itself serializes.
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;

                impl de::Visitor<'_> for Visitor {
                    type Value = $name;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str(concat!($what, " as a decimal string"))
                    }

                    fn visit_str<E: de::Error>(self, s: &str) -> Result<$name, E> {
                        s.parse().map_err(E::custom)
                    }
                }

                deserializer.deserialize_str(Visitor)
            }
        }
    };
}

decimal_newtype!(Price, "a price");
decimal_newtype!(Quantity, "a size");

impl Quantity {
    pub const ZERO: Self = Self(Decimal::ZERO);

    pub fn is_zero(self) -> bool {
        self.0.is_zero()
    }
}

impl Add for Quantity {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Quantity {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl AddAssign for Quantity {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Quantity {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qty(s: &str) -> Quantity {
        s.parse().unwrap()
    }

    #[test]
    fn scale_does_not_matter() {
        assert_eq!("100.0".parse::<Price>(), "100.00".parse::<Price>());
        assert!("99.99".parse::<Price>().unwrap() < "100".parse().unwrap());
    }

    #[test]
    fn sizes_add_up_exactly() {
        assert_eq!(qty("0.1") + qty("0.2"), qty("0.3"));
        assert!((qty("0.3") - qty("0.1") - qty("0.2")).is_zero());
        let mut size = qty("1.5");
        size += qty("0.25");
        size -= qty("1.75");
        assert_eq!(size, Quantity::ZERO);
    }

    #[test]
    fn too_many_digits_is_an_error() {
        assert!("0.00000000000000000000000000001".parse::<Price>().is_err());
        assert!("1e3".parse::<Price>().is_err());
    }

    #[test]
    fn a_number_on_the_wire_is_an_error() {
        assert!(serde_json::from_str::<Price>("83470.9").is_err());
        assert!(serde_json::from_str::<Price>("\"83470.9\"").is_ok());
    }
}
