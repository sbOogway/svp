// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (exchange/src/unit/qty.rs @ 12b029cf10ffb6c59684678b3cf404e2d82c8ce5),
// GPL-3.0-or-later, by the flowsurface contributors.

use serde::{Deserialize, Serialize};

/// Fixed atomic unit scale: 10^-QTY_SCALE is the smallest stored fraction.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Deserialize, Serialize,
)]
#[serde(transparent)]
pub struct Quantity {
    /// number of atomic units (atomic unit = 10^-QTY_SCALE)
    pub units: i64,
}

impl Quantity {
    /// number of decimal places of the atomic unit
    pub const QTY_SCALE: i32 = 8;
    pub const ZERO: Self = Self { units: 0 };

    pub const fn from_units(units: i64) -> Self {
        Self { units }
    }

    /// Absolute quantity, panics on `i64::MIN`.
    #[must_use]
    pub fn abs(self) -> Self {
        Self {
            units: self.units.checked_abs().expect("Qty abs overflowed"),
        }
    }

    /// Absolute difference between two quantities.
    #[must_use]
    pub fn abs_diff(self, other: Self) -> Self {
        if self.units >= other.units {
            self - other
        } else {
            other - self
        }
    }

    pub const fn is_zero(self) -> bool {
        self.units == 0
    }

    /// Lossy: convert qty to f32, may lose precision beyond `QTY_SCALE`
    #[allow(clippy::cast_possible_truncation)]
    pub fn to_f32_lossy(self) -> f32 {
        self.to_f64() as f32
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn to_f64(self) -> f64 {
        let scale = 10f64.powi(Self::QTY_SCALE);
        (self.units as f64) / scale
    }
}

impl std::ops::Add for Quantity {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            units: self
                .units
                .checked_add(rhs.units)
                .expect("Qty add overflowed"),
        }
    }
}

impl std::ops::AddAssign for Quantity {
    fn add_assign(&mut self, rhs: Self) {
        self.units = self
            .units
            .checked_add(rhs.units)
            .expect("Qty add_assign overflowed");
    }
}

impl std::ops::Sub for Quantity {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            units: self
                .units
                .checked_sub(rhs.units)
                .expect("Qty sub overflowed"),
        }
    }
}

impl std::ops::SubAssign for Quantity {
    fn sub_assign(&mut self, rhs: Self) {
        self.units = self
            .units
            .checked_sub(rhs.units)
            .expect("Qty sub_assign overflowed");
    }
}
