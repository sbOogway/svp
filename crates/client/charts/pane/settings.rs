// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (src/modal/pane/settings.rs @ ab8f3c1), GPL-3.0-or-later,
// by the flowsurface contributors.

//! The settings modal of each kind of pane, and the ladder's tick size.

use std::time::Duration;

use iced::{
    Alignment, Element, Length, Theme,
    widget::{
        button, checkbox, column, container, pane_grid, pick_list, radio, row, rule, slider, text,
        tooltip::Position,
    },
};

use super::Message;
use crate::charts::{
    model::{
        TickMultiplier, format_with_commas, ladder,
        timeandsales::{self, StackedBar, StackedBarRatio},
    },
    ui::{
        classic_slider_row, labeled_slider, split_column,
        style::{self, text_size},
        tooltip,
    },
};

fn cfg_view_container<'a>(
    max_width: u32,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(content)
        .width(Length::Shrink)
        .padding(28)
        .max_width(max_width)
        .style(style::chart_modal)
        .into()
}

fn info(tip: &str) -> Element<'_, Message> {
    tooltip(
        button("i").style(style::button::info),
        Some(tip),
        Position::Top,
    )
}

/// "Keep trades for", whole minutes from 1 to 60.
fn retention_slider<'a>(
    retention: Duration,
    on_change: impl Fn(Duration) -> Message + 'a,
) -> Element<'a, Message> {
    let minutes = (retention.as_secs_f32() / 60.0).max(1.0).round();
    let slider = slider(1.0..=60.0, minutes, move |new_minutes: f32| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let mins = new_minutes.round().max(1.0) as u64;
        on_change(Duration::from_secs(mins * 60))
    })
    .step(1.0_f32);

    classic_slider_row(
        text("Keep trades for"),
        slider.into(),
        Some(text(format!("≈ {minutes} min")).size(text_size::EMPHASIS)),
    )
}

pub fn ladder_cfg_view<'a>(cfg: ladder::Config, pane: pane_grid::Pane) -> Element<'a, Message> {
    let set = move |cfg| Message::SetLadderConfig(pane, cfg);

    let display_options = column![
        text("Display Options").size(text_size::SECTION),
        column![
            checkbox(cfg.show_spread)
                .label("Show Spread")
                .on_toggle(move |show_spread| set(ladder::Config { show_spread, ..cfg })),
            row![
                checkbox(cfg.show_chase_tracker)
                    .label("Show Chase Tracker")
                    .on_toggle(move |show_chase_tracker| set(ladder::Config {
                        show_chase_tracker,
                        ..cfg
                    })),
                info(
                    "Highlights consecutive best-price moves and fades when momentum stalls.\nCalculated using raw ungrouped data."
                ),
            ]
            .align_y(Alignment::Center)
            .spacing(4),
        ]
        .spacing(4),
    ]
    .spacing(8);

    let history_column = column![
        text("History").size(text_size::SECTION),
        retention_slider(cfg.trade_retention, move |trade_retention| {
            set(ladder::Config {
                trade_retention,
                ..cfg
            })
        }),
    ]
    .spacing(8);

    cfg_view_container(
        320,
        split_column([display_options.into(), history_column.into()])
            .spacing(12)
            .align_x(Alignment::Start),
    )
}

pub fn timesales_cfg_view<'a>(
    cfg: timeandsales::Config,
    pane: pane_grid::Pane,
) -> Element<'a, Message> {
    let set = move |cfg| Message::SetTapeConfig(pane, cfg);

    let trade_size_column = column![
        text("Size filter").size(text_size::SECTION),
        labeled_slider(
            "Trade",
            0.0..=50_000.0,
            cfg.trade_size_filter,
            move |trade_size_filter| set(timeandsales::Config {
                trade_size_filter,
                ..cfg
            }),
            |value| format!(">${}", format_with_commas(f64::from(value))),
            500.0,
        ),
    ]
    .spacing(8);

    let history_column = column![
        row![
            text("History").size(text_size::SECTION),
            info("Affects the stacked bar, colors and how much you can scroll down"),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
        retention_slider(cfg.trade_retention, move |trade_retention| {
            set(timeandsales::Config {
                trade_retention,
                ..cfg
            })
        }),
    ]
    .spacing(8);

    let enable_checkbox = checkbox(cfg.stacked_bar.is_some())
        .label("Show stacked bar")
        .on_toggle(move |shown| {
            let ratio = cfg.stacked_bar.map(StackedBar::ratio).unwrap_or_default();
            set(timeandsales::Config {
                stacked_bar: shown.then_some(StackedBar::Compact(ratio)),
                ..cfg
            })
        });

    let mut stacked_bar = column![enable_checkbox]
        .width(Length::Fill)
        .padding(4)
        .spacing(8);

    if let Some(bar) = cfg.stacked_bar {
        let ratio = bar.ratio();
        let is_compact = matches!(bar, StackedBar::Compact(_));
        let with_bar = move |bar| {
            set(timeandsales::Config {
                stacked_bar: Some(bar),
                ..cfg
            })
        };

        let compact = radio("Compact", true, Some(is_compact), move |_| {
            with_bar(StackedBar::Compact(ratio))
        })
        .spacing(4);
        let full = radio("Full", false, Some(is_compact), move |_| {
            with_bar(StackedBar::Full(ratio))
        })
        .spacing(4);
        let metric = pick_list(StackedBarRatio::ALL, Some(ratio), move |new_ratio| {
            with_bar(if is_compact {
                StackedBar::Compact(new_ratio)
            } else {
                StackedBar::Full(new_ratio)
            })
        });

        stacked_bar = stacked_bar.push(
            column![
                rule::horizontal(1),
                text("Mode").size(text_size::BODY),
                row![compact, full].spacing(12),
                text("Metric").size(text_size::BODY),
                metric,
            ]
            .spacing(8),
        );
    }

    let stacked_bar = container(stacked_bar)
        .style(style::modal_container)
        .padding(8);

    cfg_view_container(
        320,
        split_column([
            trade_size_column.into(),
            history_column.into(),
            stacked_bar.into(),
        ])
        .spacing(12)
        .align_x(Alignment::Start),
    )
}

/// The ladder's rows as multiples of the instrument's tick.
pub fn tick_size_view<'a>(pane: pane_grid::Pane, selected: TickMultiplier) -> Element<'a, Message> {
    let mut grid = column![text("Tick size").size(text_size::SECTION)].spacing(4);
    for multipliers in TickMultiplier::ALL.chunks(3) {
        let mut buttons = row![].spacing(4);
        for &multiplier in multipliers {
            let is_selected = selected == multiplier;
            buttons = buttons.push(
                button(
                    text(multiplier.to_string())
                        .font(style::AZERET_MONO)
                        .align_x(Alignment::Center),
                )
                .width(Length::Fill)
                .on_press(Message::SetTickMultiplier(pane, multiplier))
                .style(move |theme: &Theme, status| {
                    style::button::menu_body(theme, status, is_selected)
                }),
            );
        }
        grid = grid.push(buttons);
    }

    container(grid)
        .width(200)
        .padding(16)
        .style(style::chart_modal)
        .into()
}
