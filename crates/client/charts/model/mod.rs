// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (data/src/util.rs, exchange/src/lib.rs @ ab8f3c1), GPL-3.0-or-later,
// by the flowsurface contributors.

//! What the charts compute, without iced: each module turns the feed's
//! books and trades into what one kind of pane draws.

pub mod ladder;
pub mod timeandsales;

use std::fmt;

use svp_common::protocol::{self, Price, PriceStep, Quantity, Side};

/// A trade as the panes keep it: flowsurface's, in UNIX milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trade {
    pub time: u64,
    pub price: Price,
    pub qty: Quantity,
    /// A venue that doesn't say which side took liquidity counts as a buy.
    pub is_sell: bool,
}

impl From<&protocol::Trade> for Trade {
    fn from(trade: &protocol::Trade) -> Self {
        Self {
            time: trade.ts / 1_000_000,
            price: trade.price,
            qty: trade.size,
            is_sell: trade.aggressor == Some(Side::Sell),
        }
    }
}

impl Trade {
    /// In USD, what the size filters compare against.
    pub fn notional(&self) -> f64 {
        self.price.to_f64() * self.qty.to_f64()
    }
}

/// How many ticks one row of a grouped price axis spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TickMultiplier(pub u16);

impl fmt::Display for TickMultiplier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}x", self.0)
    }
}

impl TickMultiplier {
    pub const ALL: [TickMultiplier; 9] = [
        TickMultiplier(1),
        TickMultiplier(2),
        TickMultiplier(5),
        TickMultiplier(10),
        TickMultiplier(25),
        TickMultiplier(50),
        TickMultiplier(100),
        TickMultiplier(200),
        TickMultiplier(500),
    ];

    pub fn multiply_step(self, base_step: PriceStep) -> PriceStep {
        let units = base_step
            .units
            .checked_mul(i64::from(self.0.max(1)))
            .expect("tick multiplier overflowed PriceStep");
        PriceStep { units }
    }
}

/// The smallest step a price shown with `decimals` decimals moves by.
pub fn min_tick(decimals: u8) -> PriceStep {
    let decimals = i32::from(decimals).min(Price::ATOMIC_SCALE);
    PriceStep {
        units: 10_i64.pow(u32::try_from(Price::ATOMIC_SCALE - decimals).unwrap_or(0)),
    }
}

pub fn abbr_large_numbers(value: f64) -> String {
    let abs_value = value.abs();
    let sign = if value < 0.0 { "-" } else { "" };

    match abs_value {
        v if v >= 1_000_000_000.0 => {
            format!("{}{:.3}b", sign, v / 1_000_000_000.0)
        }
        v if v >= 1_000_000.0 => format!("{}{:.2}m", sign, v / 1_000_000.0),
        v if v >= 10_000.0 => format!("{}{:.1}k", sign, v / 1_000.0),
        v if v >= 1_000.0 => format!("{}{:.2}k", sign, v / 1_000.0),
        v if v >= 100.0 => format!("{sign}{v:.0}"),
        v if v >= 10.0 => format!("{sign}{v:.1}"),
        v if v >= 1.0 => format!("{sign}{v:.2}"),
        v if v >= 0.001 => format!("{sign}{v:.3}"),
        v if v >= 0.0001 => format!("{sign}{v:.4}"),
        v if v >= 0.00001 => format!("{sign}{v:.5}"),
        _ => {
            if abs_value == 0.0 {
                "0".to_string()
            } else {
                let s = format!("{sign}{abs_value:.3}");
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            }
        }
    }
}

/// `1234567.8` as `1,234,568`.
pub fn format_with_commas(num: f64) -> String {
    #[allow(clippy::cast_possible_truncation)]
    let rounded = num.round() as i64;
    let digits = rounded.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if rounded < 0 {
        out.insert(0, '-');
    }
    out
}

/// `HH:MM:SS.mmm` UTC of a time in UNIX milliseconds.
pub fn clock(ms: u64) -> String {
    let (h, m, s) = (ms / 3_600_000 % 24, ms / 60_000 % 60, ms / 1000 % 60);
    format!("{h:02}:{m:02}:{s:02}.{:03}", ms % 1000)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn px(s: &str) -> Price {
        s.parse().unwrap()
    }

    pub fn qty(s: &str) -> Quantity {
        s.parse().unwrap()
    }

    pub fn step(s: &str) -> PriceStep {
        s.parse().unwrap()
    }

    pub fn trade(time: u64, price: &str, size: &str, is_sell: bool) -> Trade {
        Trade {
            time,
            price: px(price),
            qty: qty(size),
            is_sell,
        }
    }

    #[test]
    fn protocol_trades_become_milliseconds_and_a_side() {
        let trade = protocol::Trade {
            instrument: "A".into(),
            ts: 1_758_796_496_123_456_789,
            price: px("100"),
            size: qty("2"),
            aggressor: None,
            id: "t".into(),
        };
        let kept = Trade::from(&trade);
        assert_eq!(kept.time, 1_758_796_496_123);
        assert!(!kept.is_sell);
        assert!((kept.notional() - 200.0).abs() < f64::EPSILON);
        assert!(
            Trade::from(&protocol::Trade {
                aggressor: Some(Side::Sell),
                ..trade
            })
            .is_sell
        );
    }

    #[test]
    fn min_tick_follows_the_decimals_shown() {
        assert_eq!(min_tick(2), step("0.01"));
        assert_eq!(min_tick(0), step("1"));
        assert_eq!(TickMultiplier(5).multiply_step(min_tick(2)), step("0.05"));
    }

    #[test]
    fn large_numbers_are_abbreviated_as_in_flowsurface() {
        assert_eq!(abbr_large_numbers(0.0), "0");
        assert_eq!(abbr_large_numbers(0.000_001_2), "0");
        assert_eq!(abbr_large_numbers(0.0123), "0.012");
        assert_eq!(abbr_large_numbers(1.5), "1.50");
        assert_eq!(abbr_large_numbers(12.34), "12.3");
        assert_eq!(abbr_large_numbers(123.4), "123");
        assert_eq!(abbr_large_numbers(1234.0), "1.23k");
        assert_eq!(abbr_large_numbers(12_345.0), "12.3k");
        assert_eq!(abbr_large_numbers(-2_500_000.0), "-2.50m");
        assert_eq!(abbr_large_numbers(3_000_000_000.0), "3.000b");
    }

    #[test]
    fn commas_group_thousands() {
        assert_eq!(format_with_commas(0.0), "0");
        assert_eq!(format_with_commas(999.0), "999");
        assert_eq!(format_with_commas(50_000.0), "50,000");
        assert_eq!(format_with_commas(-1_234_567.8), "-1,234,568");
    }

    #[test]
    fn clock_is_utc_time_of_day() {
        assert_eq!(clock(0), "00:00:00.000");
        assert_eq!(clock(1_758_796_496_123), "10:34:56.123");
    }
}
