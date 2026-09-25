// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (src/screen/dashboard/pane.rs @ ab8f3c1), GPL-3.0-or-later,
// by the flowsurface contributors.

use iced::{
    Alignment, Element, Length, Renderer, Theme, padding,
    widget::{
        Id, button, center, checkbox, column, container, pane_grid, row, text, tooltip::Position,
    },
};
use svp_common::protocol::{Instrument, Price, Quantity};

use super::{LinkGroup, Message};
use crate::charts::{
    feed::{Feed, Market, Status, Streams},
    ui::{
        PANE_CONTROL_BTN_HEIGHT, button_with_tooltip, link_group_button, picker, stack_modal,
        style::{self, Icon, icon_text},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    LinkGroup,
    Instruments,
    Settings,
    /// The pane's controls, when the title bar is too narrow for them.
    Controls,
}

#[derive(Debug, Clone)]
pub struct State {
    pub instrument: Option<String>,
    pub link_group: Option<LinkGroup>,
    pub streams: Streams,
    pub modal: Option<Modal>,
    pub query: String,
    search_id: Id,
}

impl Default for State {
    fn default() -> Self {
        Self {
            instrument: None,
            link_group: None,
            streams: Streams::ALL,
            modal: None,
            query: String::new(),
            search_id: Id::unique(),
        }
    }
}

impl State {
    /// Opens `modal`, or closes it if it is already open. Returns the
    /// search box to focus when the instrument list opens.
    pub fn show_modal(&mut self, modal: Modal) -> Option<Id> {
        if self.modal == Some(modal) {
            self.modal = None;
            return None;
        }
        self.modal = Some(modal);
        (modal == Modal::Instruments).then(|| self.search_id.clone())
    }

    pub fn view<'a>(
        &'a self,
        pane: pane_grid::Pane,
        total: usize,
        is_focused: bool,
        maximized: bool,
        feed: &'a Feed,
    ) -> pane_grid::Content<'a, Message, Theme, Renderer> {
        let show = |modal| Message::ShowModal(pane, modal);

        let instrument_button = button(
            text(self.instrument.as_deref().unwrap_or("Choose an instrument"))
                .size(style::text_size::SECTION)
                .align_y(Alignment::Center)
                .line_height(1.4),
        )
        .on_press(show(Modal::Instruments))
        .style(|theme, status| {
            style::button::modifier(theme, status, self.modal != Some(Modal::Instruments))
        })
        .height(PANE_CONTROL_BTN_HEIGHT);

        let top_left = row![
            link_group_button(self.link_group, show(Modal::LinkGroup)),
            instrument_button,
        ]
        .padding(padding::left(4))
        .align_y(Alignment::Center)
        .spacing(8)
        .height(Length::Fixed(32.0));

        let compact_control = container(
            button(
                text("...")
                    .size(style::text_size::EMPHASIS)
                    .align_y(Alignment::End),
            )
            .on_press(show(Modal::Controls))
            .style(move |theme, status| {
                style::button::transparent(
                    theme,
                    status,
                    matches!(self.modal, Some(Modal::Controls | Modal::Settings)),
                )
            }),
        )
        .align_y(Alignment::Center)
        .padding(4);

        let controls = if self.modal == Some(Modal::Controls) {
            pane_grid::Controls::new(compact_control)
        } else {
            pane_grid::Controls::dynamic(self.controls(pane, total, maximized), compact_control)
        };

        let title_bar = pane_grid::TitleBar::new(top_left)
            .controls(controls)
            .style(style::pane_title_bar);

        let body = self.with_modal(pane, total, maximized, placeholder(self, feed), feed);

        pane_grid::Content::new(body)
            .style(move |theme| style::pane_background(theme, is_focused))
            .title_bar(if self.modal.is_none() {
                title_bar
            } else {
                title_bar.always_show_controls()
            })
    }

    fn controls(
        &self,
        pane: pane_grid::Pane,
        total: usize,
        maximized: bool,
    ) -> Element<'_, Message> {
        let modal_style = |modal| {
            let is_active = self.modal == Some(modal);
            move |theme: &Theme, status| style::button::transparent(theme, status, is_active)
        };
        let control_style = |is_active| {
            move |theme: &Theme, status| style::button::transparent(theme, status, is_active)
        };

        let mut buttons = row![button_with_tooltip(
            icon_text(Icon::Cog, 12),
            Message::ShowModal(pane, Modal::Settings),
            None,
            Position::Bottom,
            modal_style(Modal::Settings),
        )];

        if total > 1 {
            let (icon, message) = if maximized {
                (Icon::ResizeSmall, Message::Restore)
            } else {
                (Icon::ResizeFull, Message::Maximize(pane))
            };
            buttons = buttons
                .push(button_with_tooltip(
                    icon_text(icon, 12),
                    message,
                    None,
                    Position::Bottom,
                    control_style(maximized),
                ))
                .push(button_with_tooltip(
                    icon_text(Icon::Close, 12),
                    Message::Close(pane),
                    None,
                    Position::Bottom,
                    control_style(false),
                ));
        }

        buttons
            .padding(padding::right(4).left(4))
            .align_y(Alignment::Center)
            .height(Length::Fixed(32.0))
            .into()
    }

    fn with_modal<'a>(
        &'a self,
        pane: pane_grid::Pane,
        total: usize,
        maximized: bool,
        base: Element<'a, Message>,
        feed: &'a Feed,
    ) -> Element<'a, Message> {
        let on_blur = Message::HideModal(pane);
        match self.modal {
            None => base,
            Some(Modal::LinkGroup) => stack_modal(
                base,
                link_group_modal(pane, self.link_group),
                on_blur,
                padding::right(12).left(4),
                Alignment::Start,
            ),
            Some(Modal::Instruments) => {
                let list = picker::view(
                    feed.instruments(),
                    &self.query,
                    self.search_id.clone(),
                    self.instrument.as_deref(),
                    move |query| Message::Search(pane, query),
                    move |instrument| Message::SelectInstrument(pane, instrument),
                );
                let content = container(list)
                    .max_width(260)
                    .padding(16)
                    .style(style::chart_modal);
                stack_modal(base, content, on_blur, padding::left(12), Alignment::Start)
            }
            Some(Modal::Settings) => stack_modal(
                base,
                self.settings(pane),
                on_blur,
                padding::right(12).left(12),
                Alignment::End,
            ),
            Some(Modal::Controls) => stack_modal(
                base,
                container(self.controls(pane, total, maximized)).style(style::chart_modal),
                on_blur,
                padding::left(12),
                Alignment::End,
            ),
        }
    }

    fn settings(&self, pane: pane_grid::Pane) -> Element<'_, Message> {
        let streams = self.streams;
        let content = column![
            text("Streams").size(style::text_size::SECTION),
            checkbox(streams.trades)
                .label("Trades")
                .on_toggle(move |trades| Message::SetStreams(pane, Streams { trades, ..streams })),
            checkbox(streams.books)
                .label("Book")
                .on_toggle(move |books| Message::SetStreams(pane, Streams { books, ..streams })),
        ]
        .spacing(8);
        container(content)
            .width(200)
            .padding(16)
            .style(style::chart_modal)
            .into()
    }
}

fn link_group_modal<'a>(
    pane: pane_grid::Pane,
    selected: Option<LinkGroup>,
) -> Element<'a, Message> {
    let mut grid = column![].spacing(4);
    for groups in LinkGroup::ALL.chunks(3) {
        let mut buttons = row![].spacing(4);
        for &group in groups {
            let is_selected = selected == Some(group);
            let label = text(group.to_string())
                .font(style::AZERET_MONO)
                .align_x(Alignment::Center);
            let style =
                move |theme: &Theme, status| style::button::menu_body(theme, status, is_selected);
            buttons = buttons.push(if is_selected {
                button_with_tooltip(
                    label,
                    Message::SwitchLinkGroup(pane, None),
                    Some("Unlink"),
                    Position::Bottom,
                    style,
                )
            } else {
                button(label)
                    .on_press(Message::SwitchLinkGroup(pane, Some(group)))
                    .style(style)
                    .into()
            });
        }
        grid = grid.push(buttons);
    }

    container(grid)
        .max_width(240)
        .padding(16)
        .style(style::chart_modal)
        .into()
}

/// Proves the data path until the pane shows a chart: the connection,
/// and for the pane's instrument the last trade, the top of the book and
/// how fast messages arrive.
fn placeholder<'a>(state: &'a State, feed: &'a Feed) -> Element<'a, Message> {
    let status = match feed.status() {
        Status::Connected { session } => text(format!("Connected, session {session}")),
        Status::Connecting { last_error: None } => text("Connecting..."),
        Status::Connecting {
            last_error: Some(error),
        } => text(format!("Connecting... ({error})")),
    }
    .size(style::text_size::BODY)
    .style(style::secondary_text);

    let body: Element<'a, Message> = match state.instrument.as_deref() {
        None => text("No instrument selected")
            .size(style::text_size::SECTION)
            .into(),
        Some(id) => match feed.market(id) {
            Some(market) => market_view(market, feed.instrument(id), state.streams),
            None if matches!(feed.status(), Status::Connected { .. }) && !feed.offers(id) => {
                text(format!("The server doesn't offer {id}"))
                    .size(style::text_size::SECTION)
                    .into()
            }
            None => text(format!("Waiting for {id}"))
                .size(style::text_size::SECTION)
                .into(),
        },
    };

    center(column![body, status].spacing(16).align_x(Alignment::Center)).into()
}

/// Prices and sizes with the decimals `instrument` shows; all of them
/// without it.
fn market_view<'a>(
    market: &'a Market,
    instrument: Option<&Instrument>,
    streams: Streams,
) -> Element<'a, Message> {
    let (price_decimals, size_decimals) = instrument.map_or((u8::MAX, u8::MAX), |i| {
        (i.coin.price_decimals(), i.coin.size_decimals())
    });
    let line = |label, value: String| {
        row![
            text(label)
                .width(80)
                .size(style::text_size::BODY)
                .style(style::secondary_text),
            text(value)
                .font(style::AZERET_MONO)
                .size(style::text_size::BODY),
        ]
        .spacing(8)
    };
    let level = |level: Option<(Price, Quantity)>| {
        level.map_or_else(
            || "-".to_owned(),
            |(price, size)| {
                format!(
                    "{} × {}",
                    price.fixed(price_decimals),
                    size.fixed(size_decimals)
                )
            },
        )
    };

    let mut lines = column![].spacing(6);
    if streams.trades {
        let last = match &market.last_trade {
            Some(trade) => row![
                text("Last trade")
                    .width(80)
                    .size(style::text_size::BODY)
                    .style(style::secondary_text),
                text(format!(
                    "{} × {}  {}",
                    trade.price.fixed(price_decimals),
                    trade.size.fixed(size_decimals),
                    clock(trade.ts)
                ))
                .font(style::AZERET_MONO)
                .size(style::text_size::BODY)
                .style(move |theme| style::side_text(theme, trade.aggressor)),
            ]
            .spacing(8),
            None => line("Last trade", "-".to_owned()),
        };
        lines = lines.push(last);
    }
    if streams.books {
        lines = lines
            .push(line("Best ask", level(market.book.best_ask())))
            .push(line("Best bid", level(market.book.best_bid())));
    }
    lines = lines.push(line("Rate", format!("{:.0} msg/s", market.rate())));
    if market.gaps > 0 {
        lines = lines.push(line(
            "Gaps",
            format!("{} ({} messages missed)", market.gaps, market.missed),
        ));
    }
    lines.into()
}

/// `HH:MM:SS.mmm` UTC of a timestamp in UNIX nanoseconds.
fn clock(ts: u64) -> String {
    let ms = ts / 1_000_000;
    let (h, m, s) = (ms / 3_600_000 % 24, ms / 60_000 % 60, ms / 1000 % 60);
    format!("{h:02}:{m:02}:{s:02}.{:03}", ms % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_utc_time_of_day() {
        assert_eq!(clock(0), "00:00:00.000");
        assert_eq!(clock(1_758_796_496_123_456_789), "10:34:56.123");
    }

    #[test]
    fn opening_the_instrument_list_focuses_its_search() {
        let mut state = State::default();
        assert_eq!(
            state.show_modal(Modal::Instruments),
            Some(state.search_id.clone())
        );
        assert_eq!(state.show_modal(Modal::Instruments), None);
        assert_eq!(state.modal, None);
        assert_eq!(state.show_modal(Modal::Settings), None);
        assert_eq!(state.modal, Some(Modal::Settings));
    }
}
