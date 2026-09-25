// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (data/src/panel/timeandsales.rs, src/screen/dashboard/panel/timeandsales.rs
// @ ab8f3c1), GPL-3.0-or-later, by the flowsurface contributors.

//! Time & Sales: the tape of recent trades, newest on top, above a bar of
//! how buys and sells split over the same minutes.

// Pixel maths in f32, as upstream draws it.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{collections::VecDeque, fmt, time::Duration};

use svp_common::protocol::Quantity;

use super::Trade;

pub const TRADE_ROW_HEIGHT: f32 = 14.0;
const METRICS_HEIGHT_COMPACT: f32 = 8.0;
const METRICS_HEIGHT_FULL: f32 = 18.0;

const TRADE_RETENTION_MS: u64 = 120_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    /// In USD: smaller trades stay in the tape's history but aren't shown.
    pub trade_size_filter: f32,
    pub trade_retention: Duration,
    pub stacked_bar: Option<StackedBar>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            trade_size_filter: 0.0,
            trade_retention: Duration::from_millis(TRADE_RETENTION_MS),
            stacked_bar: Some(StackedBar::Compact(StackedBarRatio::default())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackedBar {
    Compact(StackedBarRatio),
    /// Tall enough to write the buy and sell values on.
    Full(StackedBarRatio),
}

impl StackedBar {
    pub fn ratio(self) -> StackedBarRatio {
        match self {
            StackedBar::Compact(r) | StackedBar::Full(r) => r,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StackedBarRatio {
    Count,
    #[default]
    Volume,
    AverageSize,
}

impl fmt::Display for StackedBarRatio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StackedBarRatio::Count => write!(f, "Count"),
            StackedBarRatio::AverageSize => write!(f, "Average trade size"),
            StackedBarRatio::Volume => write!(f, "Volume"),
        }
    }
}

impl StackedBarRatio {
    pub const ALL: [StackedBarRatio; 3] = [
        StackedBarRatio::Count,
        StackedBarRatio::Volume,
        StackedBarRatio::AverageSize,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistAggValues {
    Count { buy: u64, sell: u64 },
    Qty { buy: Quantity, sell: Quantity },
}

impl HistAggValues {
    /// The buy share of the bar, a half when both are zero.
    pub fn buy_ratio(self) -> f32 {
        let (bought, sold) = match self {
            HistAggValues::Count { buy, sell } => (buy as f32, sell as f32),
            HistAggValues::Qty { buy, sell } => (buy.to_f32_lossy(), sell.to_f32_lossy()),
        };
        let total = bought + sold;
        if total > 0.0 { bought / total } else { 0.5 }
    }
}

/// Counts and sums of the tape's buys and sells, kept as trades come and go.
#[derive(Debug, Default, PartialEq, Eq)]
struct HistAgg {
    buy_count: u64,
    sell_count: u64,
    buy_sum: Quantity,
    sell_sum: Quantity,
}

impl HistAgg {
    fn average_qty(sum: Quantity, count: u64) -> Quantity {
        if count == 0 {
            return Quantity::ZERO;
        }

        let c = i64::try_from(count).unwrap_or(i64::MAX);
        let half = c / 2;
        let rounded = if sum.units >= 0 {
            sum.units.saturating_add(half).div_euclid(c)
        } else {
            sum.units.saturating_sub(half).div_euclid(c)
        };

        Quantity::from_units(rounded)
    }

    fn add(&mut self, trade: &Trade) {
        let qty = trade.qty;

        if trade.is_sell {
            self.sell_count += 1;
            self.sell_sum += qty;
        } else {
            self.buy_count += 1;
            self.buy_sum += qty;
        }
    }

    fn remove(&mut self, trade: &Trade) {
        let qty = trade.qty;

        if trade.is_sell {
            self.sell_count = self.sell_count.saturating_sub(1);
            self.sell_sum = if self.sell_sum.units >= qty.units {
                self.sell_sum - qty
            } else {
                Quantity::ZERO
            };
        } else {
            self.buy_count = self.buy_count.saturating_sub(1);
            self.buy_sum = if self.buy_sum.units >= qty.units {
                self.buy_sum - qty
            } else {
                Quantity::ZERO
            };
        }
    }

    fn values_for(&self, ratio_kind: StackedBarRatio) -> Option<HistAggValues> {
        match ratio_kind {
            StackedBarRatio::Count => {
                if self.buy_count.saturating_add(self.sell_count) == 0 {
                    return None;
                }
                Some(HistAggValues::Count {
                    buy: self.buy_count,
                    sell: self.sell_count,
                })
            }
            StackedBarRatio::Volume => {
                if self.buy_sum.units.saturating_add(self.sell_sum.units) <= 0 {
                    return None;
                }
                Some(HistAggValues::Qty {
                    buy: self.buy_sum,
                    sell: self.sell_sum,
                })
            }
            StackedBarRatio::AverageSize => {
                let buy_avg = Self::average_qty(self.buy_sum, self.buy_count);
                let sell_avg = Self::average_qty(self.sell_sum, self.sell_count);

                if buy_avg.units.saturating_add(sell_avg.units) <= 0 {
                    return None;
                }
                Some(HistAggValues::Qty {
                    buy: buy_avg,
                    sell: sell_avg,
                })
            }
        }
    }
}

/// One pane's tape. Scrolling down pauses it: new trades wait aside
/// until it scrolls back to the top.
#[derive(Debug, Default)]
pub struct TimeAndSales {
    recent_trades: VecDeque<Trade>,
    paused_trades_buffer: VecDeque<Trade>,
    hist_agg: HistAgg,
    is_paused: bool,
    max_filtered_qty: Quantity,
    config: Config,
    scroll_offset: f32,
}

impl TimeAndSales {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    /// Keeps the settings and forgets the trades, for a new instrument.
    #[must_use]
    pub fn cleared(&self) -> Self {
        Self::new(self.config)
    }

    pub fn config(&self) -> Config {
        self.config
    }

    pub fn set_config(&mut self, config: Config) {
        self.config = config;
        self.max_filtered_qty = self.max_filtered_qty();
        self.clamp_scroll();
    }

    pub fn is_empty(&self) -> bool {
        self.recent_trades.is_empty() && self.paused_trades_buffer.is_empty()
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    fn passes_filter(&self, trade: &Trade) -> bool {
        trade.notional() as f32 >= self.config.trade_size_filter
    }

    fn max_filtered_qty(&self) -> Quantity {
        self.recent_trades
            .iter()
            .filter(|t| self.passes_filter(t))
            .map(|t| t.qty)
            .fold(Quantity::ZERO, Quantity::max)
    }

    pub fn insert_buffer(&mut self, trades_buffer: &[Trade], now_ms: u64) {
        for trade in trades_buffer {
            if self.passes_filter(trade) {
                self.max_filtered_qty = self.max_filtered_qty.max(trade.qty);
            }

            if self.is_paused {
                self.paused_trades_buffer.push_back(*trade);
            } else {
                self.recent_trades.push_back(*trade);
                self.hist_agg.add(trade);
            }
        }

        self.prune(now_ms);
    }

    /// Drops what the retention no longer keeps; run every frame.
    pub fn prune(&mut self, now_ms: u64) {
        if !self.is_paused {
            self.prune_by_time(now_ms);
        }
        self.prune_paused_by_time(now_ms);
    }

    pub fn stacked_bar_height(&self) -> f32 {
        match &self.config.stacked_bar {
            Some(StackedBar::Compact(_)) => METRICS_HEIGHT_COMPACT,
            Some(StackedBar::Full(_)) => METRICS_HEIGHT_FULL,
            None => 0.0,
        }
    }

    /// The "Paused" box on top, which resumes the tape when clicked.
    pub fn pause_overlay_height(&self) -> f32 {
        self.stacked_bar_height().max(METRICS_HEIGHT_COMPACT) + TRADE_ROW_HEIGHT
    }

    pub fn scroll_offset(&self) -> f32 {
        self.scroll_offset
    }

    fn max_scroll_offset(&self) -> f32 {
        let total_content_height =
            (self.recent_trades.len() as f32 * TRADE_ROW_HEIGHT) + self.stacked_bar_height();
        (total_content_height - TRADE_ROW_HEIGHT).max(0.0)
    }

    fn clamp_scroll(&mut self) {
        self.scroll_offset = self.scroll_offset.clamp(0.0, self.max_scroll_offset());
    }

    /// `delta` in pixels, positive up, as the wheel reports it.
    pub fn scroll(&mut self, delta: f32, now_ms: u64) {
        self.scroll_offset -= delta;
        self.clamp_scroll();

        if self.scroll_offset > self.stacked_bar_height() + TRADE_ROW_HEIGHT {
            self.is_paused = true;
        } else if self.is_paused {
            self.resume(now_ms);
        }
    }

    pub fn reset_scroll(&mut self, now_ms: u64) {
        self.scroll_offset = 0.0;
        self.resume(now_ms);
    }

    fn resume(&mut self, now_ms: u64) {
        self.is_paused = false;
        for trade in &self.paused_trades_buffer {
            self.hist_agg.add(trade);
        }
        self.recent_trades
            .extend(self.paused_trades_buffer.drain(..));
        self.prune_by_time(now_ms);
    }

    pub fn hist_values(&self) -> Option<HistAggValues> {
        self.hist_agg.values_for(self.config.stacked_bar?.ratio())
    }

    /// The trades a tape `height` pixels tall shows, newest first, each
    /// with the top of its row.
    pub fn visible_trades(&self, height: f32) -> impl Iterator<Item = (f32, &Trade)> {
        let stacked_bar_h = self.stacked_bar_height();
        let content_top_y = -self.scroll_offset;
        let row_scroll_offset = (self.scroll_offset - stacked_bar_h).max(0.0);
        let start_index = (row_scroll_offset / TRADE_ROW_HEIGHT).floor() as usize;
        let visible_rows = (height / TRADE_ROW_HEIGHT).ceil() as usize;

        self.recent_trades
            .iter()
            .filter(|t| self.passes_filter(t))
            .rev()
            .skip(start_index)
            .take(visible_rows + 2)
            .enumerate()
            .map(move |(i, trade)| {
                let y =
                    content_top_y + stacked_bar_h + ((start_index + i) as f32 * TRADE_ROW_HEIGHT);
                (y, trade)
            })
            .filter(move |&(y, _)| y + TRADE_ROW_HEIGHT >= 0.0 && y <= height)
    }

    /// How strongly a row is shaded: its size against the largest shown.
    pub fn row_alpha(&self, trade: &Trade) -> f32 {
        let max = self.max_filtered_qty.to_f32_lossy();
        if max > 0.0 {
            (trade.qty.to_f32_lossy() / max).clamp(0.02, 1.0)
        } else {
            0.02
        }
    }

    /// Rows this high up hide under the "Paused" box.
    pub fn is_under_pause_overlay(&self, y: f32) -> bool {
        self.is_paused
            && y < self.stacked_bar_height().max(METRICS_HEIGHT_COMPACT) + (TRADE_ROW_HEIGHT * 0.8)
    }

    fn cutoffs(&self, now_ms: u64) -> (u64, u64) {
        let trade_retention_ms =
            u64::try_from(self.config.trade_retention.as_millis()).unwrap_or(u64::MAX);
        let prune_slack_ms = trade_retention_ms / 10;

        let low_cutoff = now_ms.saturating_sub(trade_retention_ms);
        let high_cutoff = now_ms.saturating_sub(trade_retention_ms.saturating_add(prune_slack_ms));
        (low_cutoff, high_cutoff)
    }

    fn prune_by_time(&mut self, now_ms: u64) {
        let (low_cutoff, high_cutoff) = self.cutoffs(now_ms);
        match self.recent_trades.front() {
            Some(oldest) if oldest.time < high_cutoff => {}
            _ => return,
        }

        while let Some(front) = self.recent_trades.front() {
            if front.time >= low_cutoff {
                break;
            }
            if let Some(old) = self.recent_trades.pop_front() {
                self.hist_agg.remove(&old);
            }
        }

        self.max_filtered_qty = self.max_filtered_qty();
        self.clamp_scroll();
    }

    fn prune_paused_by_time(&mut self, now_ms: u64) {
        let (low_cutoff, high_cutoff) = self.cutoffs(now_ms);
        match self.paused_trades_buffer.front() {
            Some(oldest) if oldest.time < high_cutoff => {}
            _ => return,
        }

        while let Some(front) = self.paused_trades_buffer.front() {
            if front.time >= low_cutoff {
                break;
            }
            self.paused_trades_buffer.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::tests::{qty, trade},
        *,
    };

    fn shown(tape: &TimeAndSales, height: f32) -> Vec<(f32, String)> {
        tape.visible_trades(height)
            .map(|(y, t)| (y, format!("{} {}", t.price, t.qty)))
            .collect()
    }

    fn tape(config: Config) -> TimeAndSales {
        let mut tape = TimeAndSales::new(config);
        tape.insert_buffer(
            &[
                trade(1_000, "100", "1", false),
                trade(2_000, "101", "0.5", true),
                trade(3_000, "102", "3", false),
            ],
            3_000,
        );
        tape
    }

    #[test]
    fn trades_become_rows_newest_first_below_the_bar() {
        let tape = tape(Config::default());
        let bar = tape.stacked_bar_height();
        assert_eq!(
            shown(&tape, 100.0),
            [
                (bar, "102 3".into()),
                (bar + TRADE_ROW_HEIGHT, "101 0.5".into()),
                (bar + TRADE_ROW_HEIGHT * 2.0, "100 1".into()),
            ]
        );
        let newest = tape.visible_trades(100.0).next().unwrap().1;
        assert!((tape.row_alpha(newest) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_size_filter_hides_small_trades_in_usd() {
        let mut tape = tape(Config {
            trade_size_filter: 100.0,
            stacked_bar: None,
            ..Config::default()
        });
        assert_eq!(
            shown(&tape, 100.0),
            [(0.0, "102 3".into()), (TRADE_ROW_HEIGHT, "100 1".into())],
            "$50.5 is under $100"
        );

        tape.set_config(Config {
            trade_size_filter: 301.0,
            ..tape.config()
        });
        assert_eq!(shown(&tape, 100.0), [(0.0, "102 3".into())]);
        let only = tape.visible_trades(100.0).next().unwrap().1;
        assert!((tape.row_alpha(only) - 1.0).abs() < f32::EPSILON);

        tape.set_config(Config {
            trade_size_filter: 1_000.0,
            ..tape.config()
        });
        assert!(shown(&tape, 100.0).is_empty());
        assert!(!tape.is_empty(), "filtered trades are still kept");
    }

    #[test]
    fn row_shades_scale_with_size() {
        let tape = tape(Config::default());
        let rows: Vec<f32> = tape
            .visible_trades(100.0)
            .map(|(_, t)| tape.row_alpha(t))
            .collect();
        assert!((rows[1] - 0.5 / 3.0).abs() < 1e-6);
        assert!((rows[2] - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn the_stacked_bar_splits_buys_and_sells() {
        let mut tape = tape(Config::default());
        assert_eq!(
            tape.hist_values(),
            Some(HistAggValues::Qty {
                buy: qty("4"),
                sell: qty("0.5")
            })
        );
        let ratio = tape.hist_values().unwrap().buy_ratio();
        assert!((ratio - 4.0 / 4.5).abs() < 1e-6);

        let config = |bar| Config {
            stacked_bar: Some(bar),
            ..Config::default()
        };
        tape.set_config(config(StackedBar::Full(StackedBarRatio::Count)));
        assert_eq!(
            tape.hist_values(),
            Some(HistAggValues::Count { buy: 2, sell: 1 })
        );
        assert!((tape.stacked_bar_height() - METRICS_HEIGHT_FULL).abs() < f32::EPSILON);

        tape.set_config(config(StackedBar::Compact(StackedBarRatio::AverageSize)));
        assert_eq!(
            tape.hist_values(),
            Some(HistAggValues::Qty {
                buy: qty("2"),
                sell: qty("0.5")
            })
        );

        tape.set_config(Config {
            stacked_bar: None,
            ..Config::default()
        });
        assert_eq!(tape.hist_values(), None);
        assert!(tape.stacked_bar_height().abs() < f32::EPSILON);
    }

    #[test]
    fn old_trades_leave_the_tape_and_the_bar() {
        let mut tape = tape(Config {
            trade_retention: Duration::from_secs(10),
            ..Config::default()
        });
        tape.prune(11_500);
        assert_eq!(shown(&tape, 100.0).len(), 3, "within the slack");

        tape.prune(12_500);
        assert_eq!(shown(&tape, 100.0), [(8.0, "102 3".into())]);
        assert_eq!(
            tape.hist_values(),
            Some(HistAggValues::Qty {
                buy: qty("3"),
                sell: Quantity::ZERO
            })
        );
    }

    #[test]
    fn scrolling_down_pauses_and_new_trades_wait_until_it_resumes() {
        let mut tape = tape(Config {
            stacked_bar: None,
            ..Config::default()
        });
        tape.scroll(-TRADE_ROW_HEIGHT * 1.5, 3_000);
        assert!(tape.is_paused());
        assert!(tape.is_under_pause_overlay(0.0));

        tape.insert_buffer(&[trade(4_000, "103", "1", false)], 4_000);
        assert_eq!(shown(&tape, 100.0).len(), 2, "the offset skips the newest");
        assert!(shown(&tape, 100.0).iter().all(|(_, t)| t != "103 1"));

        tape.scroll(TRADE_ROW_HEIGHT * 1.5, 4_000);
        assert!(!tape.is_paused());
        assert_eq!(shown(&tape, 100.0)[0], (0.0, "103 1".into()));
        tape.set_config(Config::default());
        assert_eq!(
            tape.hist_values(),
            Some(HistAggValues::Qty {
                buy: qty("5"),
                sell: qty("0.5")
            })
        );
    }

    #[test]
    fn scrolling_stops_at_the_last_trade_and_a_reset_resumes() {
        let mut tape = tape(Config {
            stacked_bar: None,
            ..Config::default()
        });
        tape.scroll(-1_000.0, 3_000);
        assert!((tape.scroll_offset() - TRADE_ROW_HEIGHT * 2.0).abs() < f32::EPSILON);
        assert_eq!(shown(&tape, 100.0), [(0.0, "100 1".into())]);

        tape.reset_scroll(3_000);
        assert!(!tape.is_paused());
        assert!(tape.scroll_offset().abs() < f32::EPSILON);
    }

    #[test]
    fn a_cleared_tape_keeps_its_settings() {
        let tape = tape(Config {
            trade_size_filter: 500.0,
            ..Config::default()
        });
        let cleared = tape.cleared();
        assert!(cleared.is_empty());
        assert!((cleared.config().trade_size_filter - 500.0).abs() < f32::EPSILON);
    }
}
