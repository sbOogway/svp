// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (exchange/src/unit/price.rs @ 12b029cf10ffb6c59684678b3cf404e2d82c8ce5),
// GPL-3.0-or-later, by the flowsurface contributors.

// Kept as upstream wrote it: the casts are guarded by the checks around them.
#![allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]

use serde::{Deserialize, Serialize};

/// Fixed atomic unit scale: 10^-ATOMIC_SCALE is the smallest stored fraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Price {
    /// number of atomic units (atomic unit = 10^-ATOMIC_SCALE)
    pub units: i64,
}

impl Price {
    /// number of decimal places of the atomic unit (10^-11)
    pub const ATOMIC_SCALE: i32 = 11;

    /// Round `units` to the nearest multiple of `unit`, half away from zero,
    /// saturating to the i64 range instead of wrapping at the extremes. Runs
    /// in u64 so extreme (saturating) magnitudes can never overflow.
    fn round_units_half_away(units: i64, unit: i64) -> i64 {
        let half = unit / 2;
        let mag = units.unsigned_abs();
        let rounded_mag = ((mag + half as u64) / unit as u64) * unit as u64;
        if units >= 0 {
            rounded_mag.min(i64::MAX as u64) as i64
        } else if rounded_mag > (i64::MAX as u64) + 1 {
            i64::MIN
        } else {
            (rounded_mag as i64).wrapping_neg()
        }
    }

    #[must_use]
    pub fn round_to_step(self, step: PriceStep) -> Self {
        let unit = step.units;
        if unit <= 1 {
            return self;
        }
        Self {
            units: Self::round_units_half_away(self.units, unit),
        }
    }

    /// Floor to multiple of an arbitrary step
    fn floor_to_step(self, step: PriceStep) -> Self {
        let unit = step.units;
        if unit <= 1 {
            return self;
        }
        let floored = (self.units.div_euclid(unit)) * unit;
        Self { units: floored }
    }

    /// Ceil to multiple of an arbitrary step
    fn ceil_to_step(self, step: PriceStep) -> Self {
        let unit = step.units;
        if unit <= 1 {
            return self;
        }
        let added = self.units.checked_add(unit - 1).unwrap_or_else(|| {
            if self.units.is_negative() {
                i64::MIN
            } else {
                i64::MAX
            }
        });

        let ceiled = (added.div_euclid(unit)) * unit;
        Self { units: ceiled }
    }

    /// Group with arbitrary step (e.g. sells floor, buys ceil)
    #[must_use]
    pub fn round_to_side_step(self, is_sell_or_bid: bool, step: PriceStep) -> Self {
        if is_sell_or_bid {
            self.floor_to_step(step)
        } else {
            self.ceil_to_step(step)
        }
    }

    /// Create Price from raw atomic units (no rounding)
    pub const fn from_units(units: i64) -> Self {
        Self { units }
    }

    /// Difference of two prices, saturating instead of panicking on overflow
    /// (mirrors `i64::saturating_sub`).
    #[must_use]
    pub fn saturating_sub(self, rhs: Self) -> Self {
        Self {
            units: self.units.saturating_sub(rhs.units),
        }
    }
}

impl std::ops::Add for Price {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            units: self
                .units
                .checked_add(rhs.units)
                .expect("Price add overflowed"),
        }
    }
}

impl std::ops::Sub for Price {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            units: self
                .units
                .checked_sub(rhs.units)
                .expect("Price sub overflowed"),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct PriceStep {
    /// step size in atomic units (10^-ATOMIC_SCALE)
    pub units: i64,
}
