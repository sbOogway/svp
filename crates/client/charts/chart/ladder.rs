// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (src/screen/dashboard/panel/ladder.rs @ ab8f3c1), GPL-3.0-or-later,
// by the flowsurface contributors.

//! Draws a [`Ladder`]: bid sizes, sells, price, buys, ask sizes.

use iced::{
    Alignment, Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, mouse,
    widget::canvas::{self, Canvas, Frame, Path, Stroke, Text},
};
use svp_common::protocol::{Price, Quantity};

use super::Message;
use crate::charts::{
    model::{
        abbr_large_numbers,
        ladder::{
            CHASE_CIRCLE_RADIUS, COL_PADDING, ChaseTracker, ColumnRanges, DomRow, Ladder,
            PriceGrid, ROW_HEIGHT, Side, TradedQty, column_ranges, price_layout_for,
        },
    },
    ui::style,
};

const TEXT_SIZE: f32 = style::text_size::SMALL;

pub fn view(ladder: &Ladder, price_decimals: u8) -> Element<'_, Message> {
    Canvas::new(LadderView {
        ladder,
        price_decimals,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

struct LadderView<'a> {
    ladder: &'a Ladder,
    price_decimals: u8,
}

struct Colors {
    text: Color,
    bid: Color,
    ask: Color,
}

impl canvas::Program<Message> for LadderView<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        cursor.position_in(bounds)?;

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Middle | mouse::Button::Left | mouse::Button::Right,
            )) => Some(canvas::Action::publish(Message::ResetScroll).and_capture()),
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let scroll_amount = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -(*y) * ROW_HEIGHT,
                    mouse::ScrollDelta::Pixels { y, .. } => -*y,
                };

                Some(canvas::Action::publish(Message::Scrolled(scroll_amount)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        if let Some(grid) = self.ladder.grid() {
            self.draw_ladder(&mut frame, theme, bounds, &grid);
        }
        vec![frame.into_geometry()]
    }
}

impl LadderView<'_> {
    #[allow(clippy::too_many_lines)]
    fn draw_ladder(&self, frame: &mut Frame, theme: &Theme, bounds: Rectangle, grid: &PriceGrid) {
        let palette = theme.extended_palette();
        let colors = Colors {
            text: palette.background.base.text,
            bid: palette.success.base.color,
            ask: palette.danger.base.color,
        };
        let divider_color = style::split_ruler(theme).color;

        let sample_len = self
            .format_price(grid.best_ask)
            .len()
            .max(self.format_price(grid.best_bid).len());
        let layout = price_layout_for(bounds.width, sample_len, TEXT_SIZE);
        let cols = column_ranges(bounds.width, layout.price_px);

        let (visible_rows, maxima) = self.ladder.visible_rows(bounds.height, grid);
        let best_bid = self.ladder.best_price(Side::Bid);
        let best_ask = self.ladder.best_price(Side::Ask);

        let mut spread_row: Option<(f32, f32)> = None;
        let mut best_bid_y: Option<f32> = None;
        let mut best_ask_y: Option<f32> = None;

        for visible_row in &visible_rows {
            match visible_row.row {
                DomRow::Ask { price, qty } => {
                    if Some(price) == best_ask {
                        best_ask_y = Some(visible_row.y);
                    }
                    self.draw_row(
                        frame,
                        visible_row.y,
                        (price, qty, false),
                        visible_row.traded,
                        (maxima.order_qty, maxima.trade_qty),
                        &colors,
                        &cols,
                    );
                }
                DomRow::Bid { price, qty } => {
                    if Some(price) == best_bid {
                        best_bid_y = Some(visible_row.y);
                    }
                    self.draw_row(
                        frame,
                        visible_row.y,
                        (price, qty, true),
                        visible_row.traded,
                        (maxima.order_qty, maxima.trade_qty),
                        &colors,
                        &cols,
                    );
                }
                DomRow::Spread => {
                    if let Some(spread) = self.ladder.spread() {
                        spread_row = Some((visible_row.y, visible_row.y + ROW_HEIGHT));

                        frame.fill_text(Text {
                            content: format!("Spread: {}", self.format_price(spread)),
                            position: Point::new(
                                bounds.width / 2.0,
                                visible_row.y + ROW_HEIGHT / 2.0,
                            ),
                            color: palette.secondary.strong.color,
                            size: (TEXT_SIZE - 1.0).into(),
                            font: style::AZERET_MONO,
                            align_x: Alignment::Center.into(),
                            align_y: Alignment::Center.into(),
                            ..Default::default()
                        });
                    }
                }
                DomRow::CenterDivider => {
                    let y_mid = visible_row.y + ROW_HEIGHT / 2.0 - 0.5;

                    frame.fill_rectangle(
                        Point::new(0.0, y_mid),
                        Size::new(bounds.width, 1.0),
                        divider_color,
                    );
                }
            }
        }

        if self.ladder.config.show_chase_tracker {
            let left_gap_mid_x = cols.sell.1 + f32::midpoint(layout.inside_pad_px, COL_PADDING);
            let right_gap_mid_x = cols.buy.0 - f32::midpoint(layout.inside_pad_px, COL_PADDING);

            self.draw_chase_trail(
                frame,
                grid,
                bounds,
                self.ladder.chase(Side::Bid),
                right_gap_mid_x,
                best_ask_y.map(|y| y + ROW_HEIGHT / 2.0),
                palette.success.weak.color,
                true,
            );
            self.draw_chase_trail(
                frame,
                grid,
                bounds,
                self.ladder.chase(Side::Ask),
                left_gap_mid_x,
                best_bid_y.map(|y| y + ROW_HEIGHT / 2.0),
                palette.danger.weak.color,
                false,
            );
        }

        // Price column vertical dividers with a gap over the spread row (if visible)
        let mut draw_vsplit = |x: f32, gap: Option<(f32, f32)>| {
            let x = x.floor() + 0.5;
            match gap {
                Some((top, bottom)) => {
                    if top > 0.0 {
                        frame.fill_rectangle(
                            Point::new(x, 0.0),
                            Size::new(1.0, top.max(0.0)),
                            divider_color,
                        );
                    }
                    if bottom < bounds.height {
                        frame.fill_rectangle(
                            Point::new(x, bottom),
                            Size::new(1.0, (bounds.height - bottom).max(0.0)),
                            divider_color,
                        );
                    }
                }
                None => {
                    frame.fill_rectangle(
                        Point::new(x, 0.0),
                        Size::new(1.0, bounds.height),
                        divider_color,
                    );
                }
            }
        };
        draw_vsplit(cols.sell.1, spread_row);
        draw_vsplit(cols.buy.0, spread_row);

        if let Some((top, bottom)) = spread_row {
            let y_top: f32 = top.floor() + 0.5;
            let y_bot = bottom.floor() + 0.5;

            for y in [y_top, y_bot] {
                frame.fill_rectangle(
                    Point::new(0.0, y),
                    Size::new(cols.sell.1, 1.0),
                    divider_color,
                );
                frame.fill_rectangle(
                    Point::new(cols.buy.0, y),
                    Size::new(bounds.width - cols.buy.0, 1.0),
                    divider_color,
                );
            }
        }
    }

    fn format_price(&self, price: Price) -> String {
        price.fixed(self.price_decimals).to_string()
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_row(
        &self,
        frame: &mut Frame,
        y: f32,
        (price, order_qty, is_bid): (Price, Quantity, bool),
        traded: TradedQty,
        (max_order_qty, max_trade_qty): (f32, f32),
        colors: &Colors,
        cols: &ColumnRanges,
    ) {
        let side_color = if is_bid { colors.bid } else { colors.ask };
        let order_qty_f32 = order_qty.to_f32_lossy();
        let trade_buy_qty_f32 = traded.buy.to_f32_lossy();
        let trade_sell_qty_f32 = traded.sell.to_f32_lossy();

        if is_bid {
            fill_bar(
                frame,
                cols.bid_order,
                y,
                (order_qty_f32, max_order_qty),
                side_color,
                true,
                0.20,
            );
            let qty_txt = format_quantity(order_qty);
            draw_cell_text(
                frame,
                &qty_txt,
                cols.bid_order.0 + 6.0,
                y,
                colors.text,
                Alignment::Start,
            );
        } else {
            fill_bar(
                frame,
                cols.ask_order,
                y,
                (order_qty_f32, max_order_qty),
                side_color,
                false,
                0.20,
            );
            let qty_txt = format_quantity(order_qty);
            draw_cell_text(
                frame,
                &qty_txt,
                cols.ask_order.1 - 6.0,
                y,
                colors.text,
                Alignment::End,
            );
        }

        // Sell trades (right-to-left)
        fill_bar(
            frame,
            cols.sell,
            y,
            (trade_sell_qty_f32, max_trade_qty),
            colors.ask,
            false,
            0.30,
        );
        if trade_sell_qty_f32 > 0.0 {
            draw_cell_text(
                frame,
                &format_quantity(traded.sell),
                cols.sell.1 - 6.0,
                y,
                colors.text,
                Alignment::End,
            );
        }

        // Buy trades (left-to-right)
        fill_bar(
            frame,
            cols.buy,
            y,
            (trade_buy_qty_f32, max_trade_qty),
            colors.bid,
            true,
            0.30,
        );
        if trade_buy_qty_f32 > 0.0 {
            draw_cell_text(
                frame,
                &format_quantity(traded.buy),
                cols.buy.0 + 6.0,
                y,
                colors.text,
                Alignment::Start,
            );
        }

        let price_x_center = f32::midpoint(cols.price.0, cols.price.1);
        draw_cell_text(
            frame,
            &self.format_price(price),
            price_x_center,
            y,
            side_color,
            Alignment::Center,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_chase_trail(
        &self,
        frame: &mut Frame,
        grid: &PriceGrid,
        bounds: Rectangle,
        tracker: &ChaseTracker,
        pos_x: f32,
        best_offer_y: Option<f32>,
        color: Color,
        is_bid: bool,
    ) {
        let radius = CHASE_CIRCLE_RADIUS;
        let Some((start_p_raw, end_p_raw, alpha)) = tracker.segment() else {
            return;
        };
        let start_p = start_p_raw.round_to_side_step(is_bid, grid.tick);
        let end_p = end_p_raw.round_to_side_step(is_bid, grid.tick);

        let color = color.scale_alpha(alpha);
        let stroke_w = 2.0;
        let pad_to_circle = radius + stroke_w * 0.5;

        let start_y = self.ladder.price_to_screen_y(start_p, grid, bounds.height);
        let end_y = self
            .ladder
            .price_to_screen_y(end_p, grid, bounds.height)
            .or(best_offer_y);

        if let Some(end_y) = end_y {
            if let Some(start_y) = start_y {
                let dy = end_y - start_y;
                if dy.abs() > pad_to_circle {
                    let line_end_y = end_y - dy.signum() * pad_to_circle;
                    let line_path =
                        Path::line(Point::new(pos_x, start_y), Point::new(pos_x, line_end_y));
                    frame.stroke(
                        &line_path,
                        Stroke::default().with_color(color).with_width(stroke_w),
                    );
                }
            }

            let circle = &Path::circle(Point::new(pos_x, end_y), radius);
            frame.fill(circle, color);
        }
    }
}

fn format_quantity(qty: Quantity) -> String {
    abbr_large_numbers(qty.to_f64())
}

fn fill_bar(
    frame: &mut Frame,
    (x_start, x_end): (f32, f32),
    y: f32,
    (value, scale_value_max): (f32, f32),
    color: Color,
    from_left: bool,
    alpha: f32,
) {
    if scale_value_max <= 0.0 || value <= 0.0 {
        return;
    }
    let col_width = x_end - x_start;

    let bar_width = ((value / scale_value_max) * col_width.max(1.0)).min(col_width);
    let bar_x = if from_left {
        x_start
    } else {
        x_end - bar_width
    };

    frame.fill_rectangle(
        Point::new(bar_x, y),
        Size::new(bar_width, ROW_HEIGHT),
        Color { a: alpha, ..color },
    );
}

fn draw_cell_text(
    frame: &mut Frame,
    text: &str,
    x_anchor: f32,
    y: f32,
    color: Color,
    align: Alignment,
) {
    frame.fill_text(Text {
        content: text.to_string(),
        position: Point::new(x_anchor, y + ROW_HEIGHT / 2.0),
        color,
        size: TEXT_SIZE.into(),
        font: style::AZERET_MONO,
        align_x: align.into(),
        align_y: Alignment::Center.into(),
        ..Default::default()
    });
}
