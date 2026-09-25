//! Prices and sizes as whole numbers of a fixed fraction, the units
//! flowsurface draws in: a binary float can't hold most decimal prices
//! (`83470.9` isn't one), and `i64` units add up exactly. They travel as
//! integers, in every format.

mod price;
mod quantity;

use std::{fmt, str::FromStr};

pub use price::{Price, PriceStep};
pub use quantity::Quantity;

/// A decimal string that is not a whole number of units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseUnitsError(String);

impl fmt::Display for ParseUnitsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseUnitsError {}

/// Fails rather than rounds past `scale` decimals.
fn parse_units(s: &str, scale: i32) -> Result<i64, ParseUnitsError> {
    let error = |why: &str| ParseUnitsError(format!("{s:?}: {why}"));
    let scale = usize::try_from(scale).expect("scales are positive");
    let (negative, digits) = s.strip_prefix('-').map_or((false, s), |rest| (true, rest));
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty() || !(whole.bytes().chain(fraction.bytes())).all(|b| b.is_ascii_digit()) {
        return Err(error("not a decimal number"));
    }
    if fraction.len() > scale {
        return Err(error(&format!("more than {scale} decimals")));
    }
    let units = format!("{whole}{fraction:0<scale$}")
        .parse::<i64>()
        .map_err(|_| error("out of range"))?;
    Ok(if negative { -units } else { units })
}

/// Exact, without trailing zeros.
/// With `pad`, every decimal of `scale` is written, trailing zeros too.
fn fmt_units(units: i64, scale: i32, pad: bool, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let scale = u32::try_from(scale).expect("scales are positive");
    let one = 10_u64.pow(scale);
    let magnitude = units.unsigned_abs();
    if units < 0 {
        f.write_str("-")?;
    }
    write!(f, "{}", magnitude / one)?;
    let fraction = magnitude % one;
    if fraction == 0 && !pad {
        return Ok(());
    }
    let fraction = format!("{fraction:0width$}", width = scale as usize);
    if pad {
        write!(f, ".{fraction}")
    } else {
        write!(f, ".{}", fraction.trim_end_matches('0'))
    }
}

macro_rules! decimal_strings {
    ($name:ident, $scale:expr, $pad:expr) => {
        impl FromStr for $name {
            type Err = ParseUnitsError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                parse_units(s, $scale).map(|units| Self { units })
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt_units(self.units, $scale, $pad, f)
            }
        }
    };
}

decimal_strings!(Price, Price::ATOMIC_SCALE, false);
decimal_strings!(PriceStep, Price::ATOMIC_SCALE, false);
// Sizes line up in logs and columns: always all eight decimals.
decimal_strings!(Quantity, Quantity::QTY_SCALE, true);

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn px(s: &str) -> Price {
        s.parse().unwrap()
    }

    fn step(s: &str) -> PriceStep {
        s.parse().unwrap()
    }

    fn qty(s: &str) -> Quantity {
        s.parse().unwrap()
    }

    #[test]
    fn decimal_strings_are_units() {
        assert_eq!(px("83470.9"), Price::from_units(8_347_090_000_000_000));
        assert_eq!(px("0.00000000001"), Price::from_units(1));
        assert_eq!(px("-1.5"), Price::from_units(-150_000_000_000));
        assert_eq!(qty("0.25"), Quantity::from_units(25_000_000));
        assert_eq!(
            step("0.1"),
            PriceStep {
                units: 10_000_000_000
            }
        );
        assert_eq!(px("83470.90").to_string(), "83470.9");
        assert_eq!(px("-0.5").to_string(), "-0.5");
        assert_eq!(qty("12").to_string(), "12.00000000");
        assert_eq!(qty("0.02").to_string(), "0.02000000");
        assert_eq!(qty("-0.00000001").to_string(), "-0.00000001");
        assert_eq!(Quantity::ZERO.to_string(), "0.00000000");
    }

    #[test]
    fn a_string_units_cannot_hold_is_an_error() {
        assert!("0.000000000001".parse::<Price>().is_err());
        assert!("0.000000001".parse::<Quantity>().is_err());
        assert!("92233720".parse::<Price>().is_ok());
        assert!("92233721".parse::<Price>().is_err());
        for bad in ["", ".5", "1e3", "1.2.3", "+1", "1,5", "abc"] {
            assert!(bad.parse::<Price>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn equal_prices_are_one_value() {
        assert_eq!(px("100.0"), px("100.00"));
        assert!(px("99.99") < px("100"));
        assert!(px("-1") < px("0"));
        let set: HashSet<Price> = [px("100.0"), px("100.00"), px("100")].into();
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn sizes_add_up_exactly() {
        assert_eq!(qty("0.1") + qty("0.2"), qty("0.3"));
        assert!((qty("0.3") - qty("0.1") - qty("0.2")).is_zero());
        let mut size = qty("1.5");
        size += qty("0.25");
        size -= qty("1.75");
        assert_eq!(size, Quantity::ZERO);
        assert_eq!(qty("1").abs_diff(qty("1.5")), qty("0.5"));
    }

    // The values flowsurface's charts group by: a tick of 0.1, and a coarser 5.
    #[test]
    fn rounds_to_the_nearest_step_half_away_from_zero() {
        assert_eq!(px("83470.94").round_to_step(step("0.1")), px("83470.9"));
        assert_eq!(px("83470.95").round_to_step(step("0.1")), px("83471"));
        assert_eq!(px("-83470.95").round_to_step(step("0.1")), px("-83471"));
        assert_eq!(px("102.4").round_to_step(step("5")), px("100"));
        assert_eq!(px("102.5").round_to_step(step("5")), px("105"));
        assert_eq!(
            px("0.00000000007").round_to_step(PriceStep { units: 1 }),
            px("0.00000000007")
        );
    }

    // Both round to twice the step, past the range.
    #[test]
    fn rounding_saturates_at_the_extremes() {
        let step = PriceStep {
            units: 5_000_000_000_000_000_000,
        };
        assert_eq!(
            Price::from_units(i64::MAX).round_to_step(step).units,
            i64::MAX
        );
        assert_eq!(
            Price::from_units(i64::MIN).round_to_step(step).units,
            i64::MIN
        );
    }

    #[test]
    fn bids_floor_and_asks_ceil() {
        assert_eq!(
            px("83470.99").round_to_side_step(true, step("0.1")),
            px("83470.9")
        );
        assert_eq!(
            px("83470.91").round_to_side_step(false, step("0.1")),
            px("83471")
        );
        assert_eq!(
            px("83470.9").round_to_side_step(false, step("0.1")),
            px("83470.9")
        );
        assert_eq!(
            px("-0.05").round_to_side_step(true, step("0.1")),
            px("-0.1")
        );
    }

    #[test]
    fn units_are_integers_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&px("83470.9")).unwrap(),
            "8347090000000000"
        );
        assert_eq!(
            serde_json::from_str::<Quantity>("25000000").unwrap(),
            qty("0.25")
        );
        assert!(serde_json::from_str::<Price>("\"83470.9\"").is_err());
        assert!(serde_json::from_str::<Price>("83470.9").is_err());
        let bytes = rmp_serde::to_vec(&px("83470.9")).unwrap();
        assert_eq!(
            rmp_serde::from_slice::<i64>(&bytes).unwrap(),
            8_347_090_000_000_000
        );
    }
}
