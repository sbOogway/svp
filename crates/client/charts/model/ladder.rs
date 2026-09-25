// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (data/src/panel/ladder.rs, src/screen/dashboard/panel/ladder.rs @ ab8f3c1),
// GPL-3.0-or-later, by the flowsurface contributors.

//! The DOM ladder: the book grouped into rows of a [`PriceStep`] around
//! the spread, with the trades of the last minutes at each row.

// Pixel maths in f32, as upstream draws it.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{
    collections::{BTreeMap, VecDeque},
    time::Duration,
};

use svp_common::protocol::{Book, Price, PriceStep, Quantity};

use super::{TickMultiplier, Trade};

pub const ROW_HEIGHT: f32 = 16.0;

// Total width ratios must sum to 1.0
/// Uses half of the width for each side of the order quantity columns
const ORDER_QTY_COLS_WIDTH: f32 = 0.60;
/// Uses half of the width for each side of the trade quantity columns
const TRADE_QTY_COLS_WIDTH: f32 = 0.20;

pub const COL_PADDING: f32 = 4.0;
/// Used for calculating layout with texts inside the price column
const MONO_CHAR_ADVANCE: f32 = 0.62;
/// Minimum padding on each side of the price text inside the price column
const PRICE_TEXT_SIDE_PAD_MIN: f32 = 12.0;

pub const CHASE_CIRCLE_RADIUS: f32 = 4.0;
/// Maximum interval between chase updates to consider them part of the same chase
const CHASE_MIN_INTERVAL: Duration = Duration::from_millis(200);
const CHASE_MIN_VISIBLE_OPACITY: f32 = 0.15;

const TRADE_RETENTION_MS: u64 = 8 * 60_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    pub show_spread: bool,
    pub show_chase_tracker: bool,
    pub trade_retention: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_spread: false,
            show_chase_tracker: true,
            trade_retention: Duration::from_millis(TRADE_RETENTION_MS),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Bid,
    Ask,
}

impl Side {
    fn idx(self) -> usize {
        match self {
            Side::Bid => 0,
            Side::Ask => 1,
        }
    }

    fn is_bid(self) -> bool {
        matches!(self, Side::Bid)
    }
}

#[derive(Debug, Default)]
struct GroupedDepth {
    orders: BTreeMap<Price, Quantity>,
    chase: ChaseTracker,
}

impl GroupedDepth {
    fn regroup_from_raw(
        &mut self,
        levels: &BTreeMap<Price, Quantity>,
        side: Side,
        step: PriceStep,
    ) {
        self.orders.clear();
        for (price, qty) in levels {
            let grouped_price = price.round_to_side_step(side.is_bid(), step);
            *self.orders.entry(grouped_price).or_default() += *qty;
        }
    }

    fn best_price(&self, side: Side) -> Option<Price> {
        match side {
            Side::Bid => self.orders.last_key_value().map(|(p, _)| *p),
            Side::Ask => self.orders.first_key_value().map(|(p, _)| *p),
        }
    }
}

/// Bought and sold at one grouped price.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TradedQty {
    pub buy: Quantity,
    pub sell: Quantity,
}

#[derive(Debug, Default)]
struct TradeStore {
    raw: VecDeque<Trade>,
    grouped: BTreeMap<Price, TradedQty>,
}

impl TradeStore {
    fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// Floor for sells, ceil for buys, like the book's sides.
    fn add_to_side_bin(&mut self, trade: &Trade, step: PriceStep) {
        let price = trade.price.round_to_side_step(trade.is_sell, step);
        let bin = self.grouped.entry(price).or_default();
        if trade.is_sell {
            bin.sell += trade.qty;
        } else {
            bin.buy += trade.qty;
        }
    }

    fn insert_trades(&mut self, buffer: &[Trade], step: PriceStep) {
        for trade in buffer {
            self.add_to_side_bin(trade, step);
            self.raw.push_back(*trade);
        }
    }

    fn rebuild_grouped(&mut self, step: PriceStep) {
        self.grouped.clear();
        for i in 0..self.raw.len() {
            let trade = self.raw[i];
            self.add_to_side_bin(&trade, step);
        }
    }

    fn trade_qty_at(&self, price: Price) -> TradedQty {
        self.grouped.get(&price).copied().unwrap_or_default()
    }

    fn price_range(&self) -> Option<(Price, Price)> {
        Some((
            *self.grouped.first_key_value()?.0,
            *self.grouped.last_key_value()?.0,
        ))
    }

    /// Returns true if it removed trades and regrouped.
    fn maybe_cleanup(&mut self, now_ms: u64, retention: Duration, step: PriceStep) -> bool {
        let Some(oldest) = self.raw.front() else {
            return false;
        };

        let retention_ms = u64::try_from(retention.as_millis()).unwrap_or(u64::MAX);
        if retention_ms == 0 {
            return false;
        }

        // ~1/10th of retention, min 5s
        let cleanup_step_ms = (retention_ms / 10).max(5_000);
        let threshold_ms = retention_ms + cleanup_step_ms;
        if now_ms.saturating_sub(oldest.time) < threshold_ms {
            return false;
        }

        let keep_from_ms = now_ms.saturating_sub(retention_ms);
        let mut removed = 0usize;
        while let Some(trade) = self.raw.front() {
            if trade.time < keep_from_ms {
                self.raw.pop_front();
                removed += 1;
            } else {
                break;
            }
        }

        if removed > 0 {
            self.rebuild_grouped(step);
            return true;
        }
        false
    }
}

#[derive(Debug, Clone, Copy, Default)]
enum ChaseProgress {
    #[default]
    Idle,
    Chasing {
        direction: Direction,
        start: Price,
        end: Price,
        /// Number of consecutive moves in the current direction
        consecutive: u32,
    },
    Fading {
        direction: Direction,
        start: Price,
        end: Price,
        /// Consecutive count at the moment fading started
        start_consecutive: u32,
        /// How many unchanged updates we have been fading
        fade_steps: u32,
    },
}

/// Follows the best price of one side while it keeps moving away from
/// the spread, and fades the trail once it stalls or turns back.
#[derive(Debug, Default)]
pub struct ChaseTracker {
    /// Last known best price (raw ungrouped)
    last_best: Option<Price>,
    state: ChaseProgress,
    last_update_ms: Option<u64>,
}

impl ChaseTracker {
    #[allow(clippy::too_many_lines)]
    fn update(
        &mut self,
        current_best: Option<Price>,
        is_bid: bool,
        now_ms: u64,
        max_interval: Duration,
    ) {
        let max_ms = u64::try_from(max_interval.as_millis()).unwrap_or(u64::MAX);
        if let Some(prev) = self.last_update_ms
            && max_ms > 0
            && now_ms.saturating_sub(prev) > max_ms
        {
            self.reset();
        }

        self.last_update_ms = Some(now_ms);

        let Some(current) = current_best else {
            self.reset();
            return;
        };

        if let Some(last) = self.last_best {
            let direction = if is_bid {
                Direction::Up
            } else {
                Direction::Down
            };

            let is_continue = match direction {
                Direction::Up => current > last,
                Direction::Down => current < last,
            };
            let is_reverse = match direction {
                Direction::Up => current < last,
                Direction::Down => current > last,
            };
            let is_unchanged = current == last;

            self.state = match (&self.state, is_continue, is_reverse, is_unchanged) {
                // Continue in same direction while already chasing: extend chase
                (
                    ChaseProgress::Chasing {
                        direction: sdir,
                        start,
                        consecutive,
                        ..
                    },
                    true,
                    _,
                    _,
                ) if *sdir == direction => ChaseProgress::Chasing {
                    direction,
                    start: *start,
                    end: current,
                    consecutive: consecutive.saturating_add(1),
                },
                // Start or restart a chase (from idle or from fading)
                (ChaseProgress::Idle | ChaseProgress::Fading { .. }, true, _, _) => {
                    ChaseProgress::Chasing {
                        direction,
                        start: last,
                        end: current,
                        consecutive: 1,
                    }
                }
                // Reversal or unchanged while chasing -> start fading from the
                // last chase extreme (freeze end)
                (
                    ChaseProgress::Chasing {
                        direction: sdir,
                        start,
                        end,
                        consecutive,
                    },
                    _,
                    true,
                    _,
                )
                | (
                    ChaseProgress::Chasing {
                        direction: sdir,
                        start,
                        end,
                        consecutive,
                    },
                    _,
                    _,
                    true,
                ) if *consecutive > 0 => ChaseProgress::Fading {
                    direction: *sdir,
                    start: *start,
                    end: *end,
                    start_consecutive: *consecutive,
                    fade_steps: 0,
                },
                // Unchanged or reversal while fading -> keep fading (decay)
                (
                    ChaseProgress::Fading {
                        direction: sdir,
                        start,
                        end,
                        start_consecutive,
                        fade_steps,
                    },
                    _,
                    _,
                    true,
                )
                | (
                    ChaseProgress::Fading {
                        direction: sdir,
                        start,
                        end,
                        start_consecutive,
                        fade_steps,
                    },
                    _,
                    true,
                    _,
                ) => ChaseProgress::Fading {
                    direction: *sdir,
                    start: *start,
                    end: *end,
                    start_consecutive: *start_consecutive,
                    fade_steps: fade_steps.saturating_add(1),
                },
                // Unchanged when idle -> no change
                (ChaseProgress::Idle, _, _, true) => ChaseProgress::Idle,
                _ => self.state,
            };

            if let ChaseProgress::Fading {
                start_consecutive,
                fade_steps,
                ..
            } = self.state
            {
                let base = Self::consecutive_to_alpha(start_consecutive);
                let alpha = base / (1.0 + fade_steps as f32);
                if alpha < CHASE_MIN_VISIBLE_OPACITY {
                    self.state = ChaseProgress::Idle;
                }
            }
        }

        self.last_best = Some(current);
    }

    fn reset(&mut self) {
        self.last_best = None;
        self.state = ChaseProgress::Idle;
        self.last_update_ms = None;
    }

    /// Maps consecutive steps n to [0,1): 1 - 1/(1+n)
    fn consecutive_to_alpha(n: u32) -> f32 {
        let nf = n as f32;
        1.0 - 1.0 / (1.0 + nf)
    }

    /// Where the chase started and got to (raw prices), and its opacity.
    pub fn segment(&self) -> Option<(Price, Price, f32)> {
        match self.state {
            ChaseProgress::Chasing {
                start,
                end,
                consecutive,
                ..
            } => Some((start, end, Self::consecutive_to_alpha(consecutive))),
            ChaseProgress::Fading {
                start,
                end,
                start_consecutive,
                fade_steps,
                ..
            } => {
                let alpha = {
                    let base = Self::consecutive_to_alpha(start_consecutive);
                    base / (1.0 + fade_steps as f32)
                };
                Some((start, end, alpha))
            }
            ChaseProgress::Idle => None,
        }
    }
}

/// One pane's ladder. The step follows the instrument's tick and the
/// pane's [`TickMultiplier`]; everything else comes from [`Ladder::update`].
#[derive(Debug)]
pub struct Ladder {
    pub config: Config,
    multiplier: TickMultiplier,
    step: Option<PriceStep>,
    scroll_px: f32,
    last_book_ms: Option<u64>,
    orderbook: [GroupedDepth; 2],
    trades: TradeStore,
    raw_price_spread: Option<Price>,
}

impl Default for Ladder {
    fn default() -> Self {
        Self::new(Config::default(), TickMultiplier(1))
    }
}

impl Ladder {
    pub fn new(config: Config, multiplier: TickMultiplier) -> Self {
        Self {
            config,
            multiplier,
            step: None,
            scroll_px: 0.0,
            last_book_ms: None,
            orderbook: [GroupedDepth::default(), GroupedDepth::default()],
            trades: TradeStore::default(),
            raw_price_spread: None,
        }
    }

    /// Keeps the settings and forgets the data, for a new instrument.
    #[must_use]
    pub fn cleared(&self) -> Self {
        Self::new(self.config, self.multiplier)
    }

    pub fn multiplier(&self) -> TickMultiplier {
        self.multiplier
    }

    /// Takes effect with the next [`Ladder::update`], which knows the tick.
    pub fn set_multiplier(&mut self, multiplier: TickMultiplier) {
        self.multiplier = multiplier;
    }

    pub fn step(&self) -> Option<PriceStep> {
        self.step
    }

    pub fn set_config(&mut self, config: Config) {
        if !config.show_chase_tracker {
            self.chase_mut(Side::Bid).reset();
            self.chase_mut(Side::Ask).reset();
        }
        self.config = config;
    }

    /// Adds the trades of a frame, and regroups the book when it changed
    /// since `book_ms` last did or the step changed. `min_tick` is the
    /// instrument's.
    pub fn update(&mut self, min_tick: PriceStep, book: &Book, book_ms: u64, trades: &[Trade]) {
        let step = self.multiplier.multiply_step(min_tick);
        let step_changed = self.step != Some(step);
        if step_changed {
            self.step = Some(step);
            self.trades.rebuild_grouped(step);
        }
        self.trades.insert_trades(trades, step);

        if self.last_book_ms != Some(book_ms) {
            self.insert_depth(book, book_ms, step);
        } else if step_changed {
            self.regroup_from_depth(book, step);
        }
    }

    fn insert_depth(&mut self, book: &Book, update_ms: u64, step: PriceStep) {
        let raw_best_bid = book.bids().last_key_value().map(|(p, _)| *p);
        let raw_best_ask = book.asks().first_key_value().map(|(p, _)| *p);
        self.raw_price_spread = match (raw_best_bid, raw_best_ask) {
            (Some(bid), Some(ask)) => Some(ask.saturating_sub(bid)),
            _ => None,
        };

        if self.config.show_chase_tracker {
            let max_int = CHASE_MIN_INTERVAL;
            self.chase_mut(Side::Bid)
                .update(raw_best_bid, true, update_ms, max_int);
            self.chase_mut(Side::Ask)
                .update(raw_best_ask, false, update_ms, max_int);
        } else {
            self.chase_mut(Side::Bid).reset();
            self.chase_mut(Side::Ask).reset();
        }

        self.trades
            .maybe_cleanup(update_ms, self.config.trade_retention, step);

        self.regroup_from_depth(book, step);
        self.last_book_ms = Some(update_ms);
    }

    fn regroup_from_depth(&mut self, book: &Book, step: PriceStep) {
        self.orderbook[Side::Ask.idx()].regroup_from_raw(book.asks(), Side::Ask, step);
        self.orderbook[Side::Bid.idx()].regroup_from_raw(book.bids(), Side::Bid, step);
    }

    pub fn is_empty(&self) -> bool {
        self.grouped(Side::Ask).is_empty()
            && self.grouped(Side::Bid).is_empty()
            && self.trades.is_empty()
    }

    fn grouped(&self, side: Side) -> &BTreeMap<Price, Quantity> {
        &self.orderbook[side.idx()].orders
    }

    pub fn chase(&self, side: Side) -> &ChaseTracker {
        &self.orderbook[side.idx()].chase
    }

    fn chase_mut(&mut self, side: Side) -> &mut ChaseTracker {
        &mut self.orderbook[side.idx()].chase
    }

    pub fn best_price(&self, side: Side) -> Option<Price> {
        self.orderbook[side.idx()].best_price(side)
    }

    /// The raw book's, ungrouped.
    pub fn spread(&self) -> Option<Price> {
        self.raw_price_spread
    }

    pub fn trade_qty_at(&self, price: Price) -> TradedQty {
        self.trades.trade_qty_at(price)
    }

    pub fn scroll(&mut self, delta: f32) {
        self.scroll_px += delta;
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_px = 0.0;
    }

    /// The rows around the spread: the best bid, or failing it the best
    /// ask, or failing both the middle of the trades seen.
    pub fn grid(&self) -> Option<PriceGrid> {
        let step = self.step?;
        let best_bid = match (self.best_price(Side::Bid), self.best_price(Side::Ask)) {
            (Some(bb), _) => bb,
            (None, Some(ba)) => ba.add_steps(-1, step),
            (None, None) => {
                let (min_t, max_t) = self.trades.price_range()?;
                let steps = Price::steps_between_inclusive(min_t, max_t, step).unwrap_or(1);
                max_t.add_steps(-(steps as i64 / 2), step)
            }
        };
        let best_ask = best_bid.add_steps(1, step);

        Some(PriceGrid {
            best_bid,
            best_ask,
            tick: step,
        })
    }

    /// The rows a ladder `height` pixels tall shows, top to bottom, and the
    /// largest sizes among them, which scale the bars.
    pub fn visible_rows(&self, height: f32, grid: &PriceGrid) -> (Vec<VisibleRow>, Maxima) {
        let asks_grouped = self.grouped(Side::Ask);
        let bids_grouped = self.grouped(Side::Bid);

        let mut visible: Vec<VisibleRow> = Vec::new();
        let mut maxima = Maxima::default();

        let mid_screen_y = height * 0.5;
        let scroll = self.scroll_px;

        let y0 = mid_screen_y + PriceGrid::top_y(0) - scroll;
        let idx_top = ((0.0 - y0) / ROW_HEIGHT).floor() as i32;

        let rows_needed = (height / ROW_HEIGHT).ceil() as i32 + 1;
        let idx_bottom = idx_top + rows_needed;

        for idx in idx_top..=idx_bottom {
            if idx == 0 {
                let top_y_screen = mid_screen_y + PriceGrid::top_y(0) - scroll;
                if top_y_screen < height && top_y_screen + ROW_HEIGHT > 0.0 {
                    let row = if self.config.show_spread {
                        DomRow::Spread
                    } else {
                        DomRow::CenterDivider
                    };

                    visible.push(VisibleRow {
                        row,
                        y: top_y_screen,
                        traded: TradedQty::default(),
                    });
                }
                continue;
            }

            let Some(price) = grid.index_to_price(idx) else {
                continue;
            };

            let is_bid = idx > 0;
            let order_qty = if is_bid {
                bids_grouped.get(&price).copied().unwrap_or_default()
            } else {
                asks_grouped.get(&price).copied().unwrap_or_default()
            };

            let top_y_screen = mid_screen_y + PriceGrid::top_y(idx) - scroll;
            if top_y_screen >= height || top_y_screen + ROW_HEIGHT <= 0.0 {
                continue;
            }

            maxima.order_qty = maxima.order_qty.max(order_qty.to_f32_lossy());
            let traded = self.trade_qty_at(price);
            maxima.trade_qty = maxima
                .trade_qty
                .max(traded.buy.to_f32_lossy().max(traded.sell.to_f32_lossy()));

            let row = if is_bid {
                DomRow::Bid {
                    price,
                    qty: order_qty,
                }
            } else {
                DomRow::Ask {
                    price,
                    qty: order_qty,
                }
            };

            visible.push(VisibleRow {
                row,
                y: top_y_screen,
                traded,
            });
        }

        visible.sort_by(|a, b| a.y.total_cmp(&b.y));
        (visible, maxima)
    }

    /// The middle of `price`'s row, when the grid reaches it.
    pub fn price_to_screen_y(&self, price: Price, grid: &PriceGrid, height: f32) -> Option<f32> {
        let mid_screen_y = height * 0.5;
        let scroll = self.scroll_px;

        let idx = if price >= grid.best_ask {
            let steps = Price::steps_between_inclusive(grid.best_ask, price, grid.tick)?;
            -(steps as i32)
        } else if price <= grid.best_bid {
            let steps = Price::steps_between_inclusive(price, grid.best_bid, grid.tick)?;
            steps as i32
        } else {
            return Some(mid_screen_y - scroll);
        };

        let y = mid_screen_y + PriceGrid::top_y(idx) - scroll + ROW_HEIGHT / 2.0;
        Some(y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomRow {
    Ask { price: Price, qty: Quantity },
    Spread,
    CenterDivider,
    Bid { price: Price, qty: Quantity },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibleRow {
    pub row: DomRow,
    pub y: f32,
    pub traded: TradedQty,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Maxima {
    pub order_qty: f32,
    pub trade_qty: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceGrid {
    pub best_bid: Price,
    pub best_ask: Price,
    pub tick: PriceStep,
}

impl PriceGrid {
    /// Row 0 is the spread; bids count up from 1 below it, asks down from
    /// -1 above it.
    fn index_to_price(&self, idx: i32) -> Option<Price> {
        match idx {
            0 => None,
            1.. => Some(self.best_bid.add_steps(-i64::from(idx - 1), self.tick)),
            _ => Some(self.best_ask.add_steps(i64::from(-1 - idx), self.tick)),
        }
    }

    fn top_y(idx: i32) -> f32 {
        (idx as f32) * ROW_HEIGHT - ROW_HEIGHT * 0.5
    }
}

/// `[BidOrderQty][SellQty][ Price ][BuyQty][AskOrderQty]`, as x ranges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnRanges {
    pub bid_order: (f32, f32),
    pub sell: (f32, f32),
    pub price: (f32, f32),
    pub buy: (f32, f32),
    pub ask_order: (f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceLayout {
    pub price_px: f32,
    pub inside_pad_px: f32,
}

const NUMBER_OF_COLUMN_GAPS: f32 = 4.0;

/// The price column fits the longest price, `text_len` characters of a
/// monospace font `text_size` pixels high.
pub fn price_layout_for(total_width: f32, text_len: usize, text_size: f32) -> PriceLayout {
    let text_px = (text_len as f32) * text_size * MONO_CHAR_ADVANCE;

    let desired_total_gap = CHASE_CIRCLE_RADIUS * 2.0 + 4.0;
    let inside_pad_px = PRICE_TEXT_SIDE_PAD_MIN
        .max(desired_total_gap - COL_PADDING)
        .max(0.0);

    let price_px = (text_px + 2.0 * inside_pad_px).min(total_width.max(0.0));

    PriceLayout {
        price_px,
        inside_pad_px,
    }
}

pub fn column_ranges(width: f32, price_px: f32) -> ColumnRanges {
    let total_gutter_width = COL_PADDING * NUMBER_OF_COLUMN_GAPS;
    let usable_width = (width - total_gutter_width).max(0.0);

    let price_width = price_px.min(usable_width);

    let rest = (usable_width - price_width).max(0.0);
    let rest_ratio = ORDER_QTY_COLS_WIDTH + TRADE_QTY_COLS_WIDTH;

    let order_share = (ORDER_QTY_COLS_WIDTH / rest_ratio) * rest;
    let trade_share = (TRADE_QTY_COLS_WIDTH / rest_ratio) * rest;

    let mut widths = [
        order_share * 0.5,
        trade_share * 0.5,
        price_width,
        trade_share * 0.5,
        order_share * 0.5,
    ]
    .into_iter();
    let mut cursor_x = 0.0;
    let mut next = || {
        let start = cursor_x;
        let end = start + widths.next().unwrap_or_default();
        cursor_x = end + COL_PADDING;
        (start, end)
    };

    ColumnRanges {
        bid_order: next(),
        sell: next(),
        price: next(),
        buy: next(),
        ask_order: next(),
    }
}

#[cfg(test)]
mod tests {
    use svp_common::protocol::BookData;

    use super::{
        super::{
            min_tick,
            tests::{px, qty, step, trade},
        },
        *,
    };

    fn book(bids: &[(&str, &str)], asks: &[(&str, &str)]) -> Book {
        let levels =
            |levels: &[(&str, &str)]| levels.iter().map(|&(p, q)| (px(p), qty(q))).collect();
        let mut book = Book::default();
        book.apply(&BookData::Snapshot {
            bids: levels(bids),
            asks: levels(asks),
        });
        book
    }

    fn prices(rows: &[VisibleRow]) -> Vec<String> {
        rows.iter()
            .map(|row| match row.row {
                DomRow::Ask { price, qty } => format!("A {price} {qty}"),
                DomRow::Bid { price, qty } => format!("B {price} {qty}"),
                DomRow::Spread => "spread".into(),
                DomRow::CenterDivider => "-".into(),
            })
            .collect()
    }

    // A tick of 0.1 grouped by 5 into rows of 0.5: bids floor, asks ceil.
    fn ladder() -> Ladder {
        let mut ladder = Ladder::new(Config::default(), TickMultiplier(5));
        ladder.update(
            min_tick(1),
            &book(
                &[("100.4", "1"), ("100.1", "2"), ("99.9", "4")],
                &[("100.6", "3"), ("100.9", "1"), ("101.2", "0.5")],
            ),
            1_000,
            &[],
        );
        ladder
    }

    #[test]
    fn a_book_groups_into_rows_of_the_step_around_the_spread() {
        let ladder = ladder();
        assert_eq!(ladder.step(), Some(step("0.5")));
        let grid = ladder.grid().unwrap();
        assert_eq!((grid.best_bid, grid.best_ask), (px("100"), px("100.5")));
        assert_eq!(ladder.spread(), Some(px("0.2")));

        // Nine rows: four asks, the divider, four bids.
        let (rows, maxima) = ladder.visible_rows(ROW_HEIGHT * 9.0, &grid);
        assert_eq!(
            prices(&rows),
            [
                "A 102 0",
                "A 101.5 0.5",
                "A 101 4",
                "A 100.5 0",
                "-",
                "B 100 3",
                "B 99.5 4",
                "B 99 0",
                "B 98.5 0",
            ]
        );
        assert!((maxima.order_qty - 4.0).abs() < f32::EPSILON);
        assert!(maxima.trade_qty.abs() < f32::EPSILON);
        assert!((rows[4].y - ROW_HEIGHT * 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn scrolling_moves_the_rows_and_a_click_recenters() {
        let mut ladder = ladder();
        let grid = ladder.grid().unwrap();
        ladder.scroll(-ROW_HEIGHT * 2.0);
        let (rows, _) = ladder.visible_rows(ROW_HEIGHT * 9.0, &grid);
        assert_eq!(prices(&rows)[..3], ["A 103 0", "A 102.5 0", "A 102 0"]);

        ladder.reset_scroll();
        assert_eq!(
            ladder.price_to_screen_y(px("100"), &grid, ROW_HEIGHT * 9.0),
            Some(ROW_HEIGHT * 5.5)
        );
    }

    #[test]
    fn the_spread_row_shows_when_asked() {
        let mut ladder = ladder();
        ladder.set_config(Config {
            show_spread: true,
            ..Config::default()
        });
        let grid = ladder.grid().unwrap();
        let (rows, _) = ladder.visible_rows(ROW_HEIGHT * 9.0, &grid);
        assert_eq!(rows[4].row, DomRow::Spread);
    }

    #[test]
    fn trades_land_on_their_side_bins_and_regroup_with_the_step() {
        let mut ladder = ladder();
        let b = book(&[("100.4", "1")], &[("100.6", "3")]);
        ladder.update(
            min_tick(1),
            &b,
            1_000,
            &[
                trade(1_000, "100.3", "1", false),
                trade(1_000, "100.3", "2", true),
                trade(1_000, "100.5", "0.5", false),
            ],
        );
        assert_eq!(
            ladder.trade_qty_at(px("100.5")),
            TradedQty {
                buy: qty("1.5"),
                sell: Quantity::ZERO
            }
        );
        assert_eq!(ladder.trade_qty_at(px("100")).sell, qty("2"));

        ladder.set_multiplier(TickMultiplier(10));
        ladder.update(min_tick(1), &b, 1_000, &[]);
        assert_eq!(ladder.step(), Some(step("1")));
        assert_eq!(ladder.trade_qty_at(px("101")).buy, qty("1.5"));
        assert_eq!(ladder.trade_qty_at(px("100")).sell, qty("2"));
        assert_eq!(ladder.best_price(Side::Bid), Some(px("100")));
        assert_eq!(ladder.best_price(Side::Ask), Some(px("101")));
    }

    #[test]
    fn trades_older_than_the_retention_go_with_a_later_book() {
        let mut ladder = Ladder::new(
            Config {
                trade_retention: Duration::from_secs(60),
                ..Config::default()
            },
            TickMultiplier(1),
        );
        let b = book(&[("100", "1")], &[("101", "1")]);
        ladder.update(
            min_tick(0),
            &b,
            0,
            &[trade(0, "100", "1", true), trade(30_000, "101", "1", false)],
        );
        ladder.update(min_tick(0), &b, 64_000, &[]);
        assert_eq!(
            ladder.trade_qty_at(px("100")).sell,
            qty("1"),
            "within slack"
        );

        ladder.update(min_tick(0), &b, 66_000, &[]);
        assert_eq!(ladder.trade_qty_at(px("100")), TradedQty::default());
        assert_eq!(ladder.trade_qty_at(px("101")).buy, qty("1"));
    }

    #[test]
    fn without_a_book_the_grid_centres_on_the_trades() {
        let mut ladder = Ladder::default();
        assert!(ladder.is_empty());
        assert_eq!(ladder.grid(), None);
        ladder.update(
            min_tick(0),
            &Book::default(),
            0,
            &[trade(0, "90", "1", false), trade(0, "100", "1", true)],
        );
        assert!(!ladder.is_empty());
        let grid = ladder.grid().unwrap();
        assert_eq!((grid.best_bid, grid.best_ask), (px("95"), px("96")));
    }

    #[test]
    fn the_chase_grows_with_each_move_and_fades_when_it_stalls() {
        let mut ladder = Ladder::new(Config::default(), TickMultiplier(1));
        let mut at = |ms, bid: &str| {
            ladder.update(min_tick(0), &book(&[(bid, "1")], &[("200", "1")]), ms, &[]);
            ladder.chase(Side::Bid).segment()
        };
        assert_eq!(at(0, "100"), None);
        assert_eq!(at(100, "101"), Some((px("100"), px("101"), 0.5)));
        let (start, end, alpha) = at(200, "102").unwrap();
        assert_eq!((start, end), (px("100"), px("102")));
        assert!((alpha - 2.0 / 3.0).abs() < f32::EPSILON);

        let (_, end, alpha) = at(300, "101").unwrap();
        assert_eq!(end, px("102"), "the end stays at the extreme");
        assert!((alpha - 2.0 / 3.0).abs() < f32::EPSILON);
        let (_, _, faded) = at(400, "101").unwrap();
        assert!((faded - 1.0 / 3.0).abs() < f32::EPSILON);

        assert!(at(1_000, "105").is_none(), "too long since the last book");
    }

    #[test]
    fn hiding_the_chase_resets_it() {
        let mut ladder = Ladder::new(Config::default(), TickMultiplier(1));
        for (ms, bid) in [(0, "100"), (100, "101")] {
            ladder.update(min_tick(0), &book(&[(bid, "1")], &[]), ms, &[]);
        }
        assert!(ladder.chase(Side::Bid).segment().is_some());
        ladder.set_config(Config {
            show_chase_tracker: false,
            ..Config::default()
        });
        assert!(ladder.chase(Side::Bid).segment().is_none());
    }

    #[test]
    fn a_cleared_ladder_keeps_its_settings() {
        let mut ladder = ladder();
        ladder.set_config(Config {
            show_spread: true,
            ..Config::default()
        });
        let cleared = ladder.cleared();
        assert!(cleared.is_empty());
        assert!(cleared.config.show_spread);
        assert_eq!(cleared.multiplier(), TickMultiplier(5));
        assert_eq!(cleared.step(), None);
    }

    #[test]
    fn columns_split_the_width_around_the_price() {
        let layout = price_layout_for(400.0, 6, 11.0);
        assert!((layout.inside_pad_px - 12.0).abs() < f32::EPSILON);
        assert!((layout.price_px - (6.0 * 11.0 * 0.62 + 24.0)).abs() < 1e-4);

        let cols = column_ranges(216.0, 40.0);
        // 200 usable, 160 left: 120 for orders, 40 for trades.
        let expected = [
            (cols.bid_order, (0.0, 60.0)),
            (cols.sell, (64.0, 84.0)),
            (cols.price, (88.0, 128.0)),
            (cols.buy, (132.0, 152.0)),
            (cols.ask_order, (156.0, 216.0)),
        ];
        for ((start, end), (want_start, want_end)) in expected {
            assert!((start - want_start).abs() < 1e-3 && (end - want_end).abs() < 1e-3);
        }
    }
}
