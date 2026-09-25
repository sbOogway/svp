// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (src/screen/dashboard/panel/timeandsales.rs @ ab8f3c1), GPL-3.0-or-later,
// by the flowsurface contributors.

//! Draws a [`TimeAndSales`]: the stacked bar, then a row per trade.

use iced::{
    Alignment, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, mouse,
    widget::canvas::{self, Canvas, Frame, Text},
};

use super::Message;
use crate::charts::{
    model::{
        abbr_large_numbers, clock,
        timeandsales::{HistAggValues, StackedBar, TRADE_ROW_HEIGHT, TimeAndSales},
    },
    ui::style::{self, darken, lighten},
};

const TEXT_SIZE: f32 = style::text_size::SMALL;

pub fn view(tape: &TimeAndSales, price_decimals: u8) -> Element<'_, Message> {
    Canvas::new(TimeAndSalesView {
        tape,
        price_decimals,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

struct TimeAndSalesView<'a> {
    tape: &'a TimeAndSales,
    price_decimals: u8,
}

impl TimeAndSalesView<'_> {
    fn paused_box(&self, bounds: Rectangle) -> Rectangle {
        Rectangle {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: self.tape.pause_overlay_height(),
        }
    }
}

impl canvas::Program<Message> for TimeAndSalesView<'_> {
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
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                Some(canvas::Action::publish(Message::ResetScroll).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if self.tape.is_paused() && cursor.is_over(self.paused_box(bounds)) =>
            {
                Some(canvas::Action::publish(Message::ResetScroll).and_capture())
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let scroll_amount = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y * TRADE_ROW_HEIGHT * 3.0,
                    mouse::ScrollDelta::Pixels { y, .. } => *y,
                };

                Some(canvas::Action::publish(Message::Scrolled(scroll_amount)).and_capture())
            }
            _ => None,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let palette = theme.extended_palette();
        let stacked_bar_h = self.tape.stacked_bar_height();
        let mut frame = Frame::new(renderer, bounds.size());
        let content_top_y = -self.tape.scroll_offset();

        if let (Some(bar), Some(values)) = (self.tape.config().stacked_bar, self.tape.hist_values())
        {
            let buy_bar_width = (bounds.width * values.buy_ratio()).round();
            let sell_bar_width = bounds.width - buy_bar_width;

            frame.fill_rectangle(
                Point::new(0.0, content_top_y),
                Size::new(buy_bar_width, stacked_bar_h),
                palette.success.weak.color,
            );
            frame.fill_rectangle(
                Point::new(buy_bar_width, content_top_y),
                Size::new(sell_bar_width, stacked_bar_h),
                palette.danger.weak.color,
            );

            if matches!(bar, StackedBar::Full(_)) {
                let (buy_text, sell_text) = match values {
                    HistAggValues::Count { buy, sell } => (buy.to_string(), sell.to_string()),
                    HistAggValues::Qty { buy, sell } => (
                        abbr_large_numbers(buy.to_f64()),
                        abbr_large_numbers(sell.to_f64()),
                    ),
                };
                let center_y = content_top_y + (stacked_bar_h / 2.0);

                frame.fill_text(Text {
                    content: buy_text,
                    position: Point::new(8.0, center_y),
                    size: TEXT_SIZE.into(),
                    font: style::AZERET_MONO,
                    color: palette.success.weak.text,
                    align_x: Alignment::Start.into(),
                    align_y: Alignment::Center.into(),
                    ..Default::default()
                });
                frame.fill_text(Text {
                    content: sell_text,
                    position: Point::new(bounds.width - 8.0, center_y),
                    size: TEXT_SIZE.into(),
                    font: style::AZERET_MONO,
                    color: palette.danger.weak.text,
                    align_x: Alignment::End.into(),
                    align_y: Alignment::Center.into(),
                    ..Default::default()
                });
            }
        }

        let row_width = bounds.width;
        let create_text =
            |content: String, position: Point, align_x: Alignment, color: iced::Color| Text {
                content,
                position,
                size: TEXT_SIZE.into(),
                font: style::AZERET_MONO,
                color,
                align_x: align_x.into(),
                ..Default::default()
            };

        for (y_position, entry) in self.tape.visible_trades(bounds.height) {
            let bg_color = if entry.is_sell {
                palette.danger.weak.color
            } else {
                palette.success.weak.color
            };

            let bg_color_alpha = self.tape.row_alpha(entry);

            let mut text_color = if palette.is_dark {
                lighten(bg_color, bg_color_alpha.max(0.1))
            } else {
                darken(bg_color, (bg_color_alpha * 0.8).max(0.1))
            };

            if self.tape.is_under_pause_overlay(y_position) {
                text_color = text_color.scale_alpha(0.1);
            }

            frame.fill_rectangle(
                Point::new(0.0, y_position),
                Size::new(row_width, TRADE_ROW_HEIGHT),
                bg_color.scale_alpha(bg_color_alpha.min(0.9)),
            );

            frame.fill_text(create_text(
                clock(entry.time),
                Point::new(row_width * 0.1, y_position),
                Alignment::Start,
                text_color,
            ));
            frame.fill_text(create_text(
                entry.price.fixed(self.price_decimals).to_string(),
                Point::new(row_width * 0.67, y_position),
                Alignment::End,
                text_color,
            ));
            frame.fill_text(create_text(
                abbr_large_numbers(entry.qty.to_f64()),
                Point::new(row_width * 0.9, y_position),
                Alignment::End,
                text_color,
            ));
        }

        if self.tape.is_paused() {
            let pause_overlay_height = self.tape.pause_overlay_height();
            let bg_color = if cursor.is_over(self.paused_box(bounds)) {
                palette.background.strong.color
            } else {
                palette.background.weak.color
            };

            frame.fill_rectangle(
                Point::ORIGIN,
                Size::new(frame.width(), pause_overlay_height),
                bg_color,
            );

            frame.fill_text(Text {
                content: "Paused".to_string(),
                position: Point::new(frame.width() * 0.5, pause_overlay_height / 2.0),
                size: 12.0.into(),
                font: style::AZERET_MONO,
                color: palette.background.strong.text,
                align_x: Alignment::Center.into(),
                align_y: Alignment::Center.into(),
                ..Default::default()
            });
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.tape.is_paused() && cursor.is_over(self.paused_box(bounds)) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}
