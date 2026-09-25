// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (src/screen/dashboard/pane.rs, data/src/layout/pane.rs @ ab8f3c1),
// GPL-3.0-or-later, by the flowsurface contributors.

use std::fmt;

use iced::{
    Alignment, Element, Length, Renderer, Theme, padding,
    widget::{
        Id, button, center, column, container, pane_grid, pick_list, row, text, tooltip::Position,
    },
};
use svp_common::protocol;

use super::{LinkGroup, Message, settings};
use crate::charts::{
    chart,
    feed::{Feed, Status, Streams},
    model::{self, ladder::Ladder, min_tick, timeandsales::TimeAndSales},
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
    TickSize,
    /// The pane's controls, when the title bar is too narrow for them.
    Controls,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Starter,
    Ladder,
    TimeAndSales,
}

impl ContentKind {
    /// What a Starter pane can become.
    pub const VIEWS: [ContentKind; 2] = [ContentKind::Ladder, ContentKind::TimeAndSales];
}

impl fmt::Display for ContentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ContentKind::Starter => "Starter Pane",
            ContentKind::TimeAndSales => "Time&Sales",
            ContentKind::Ladder => "DOM/Ladder",
        })
    }
}

#[derive(Debug)]
pub enum Content {
    Starter,
    Ladder(Ladder),
    TimeAndSales(TimeAndSales),
}

impl Content {
    pub fn new(kind: ContentKind) -> Self {
        match kind {
            ContentKind::Starter => Content::Starter,
            ContentKind::Ladder => Content::Ladder(Ladder::default()),
            ContentKind::TimeAndSales => Content::TimeAndSales(TimeAndSales::default()),
        }
    }

    pub fn kind(&self) -> ContentKind {
        match self {
            Content::Starter => ContentKind::Starter,
            Content::Ladder(_) => ContentKind::Ladder,
            Content::TimeAndSales(_) => ContentKind::TimeAndSales,
        }
    }

    /// What the content needs from the server.
    pub fn streams(&self) -> Streams {
        match self {
            Content::Starter => Streams::default(),
            Content::Ladder(_) => Streams::ALL,
            Content::TimeAndSales(_) => Streams {
                trades: true,
                books: false,
            },
        }
    }

    fn cleared(&self) -> Self {
        match self {
            Content::Starter => Content::Starter,
            Content::Ladder(ladder) => Content::Ladder(ladder.cleared()),
            Content::TimeAndSales(tape) => Content::TimeAndSales(tape.cleared()),
        }
    }
}

#[derive(Debug)]
pub struct State {
    pub instrument: Option<String>,
    pub link_group: Option<LinkGroup>,
    pub content: Content,
    pub modal: Option<Modal>,
    pub query: String,
    search_id: Id,
}

impl Default for State {
    fn default() -> Self {
        Self::new(ContentKind::Starter)
    }
}

impl State {
    pub fn new(kind: ContentKind) -> Self {
        Self {
            instrument: None,
            link_group: None,
            content: Content::new(kind),
            modal: None,
            query: String::new(),
            search_id: Id::unique(),
        }
    }

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

    /// A new instrument starts the content over, with its settings.
    pub fn set_instrument(&mut self, instrument: &str) {
        if self.instrument.as_deref() != Some(instrument) {
            self.instrument = Some(instrument.to_owned());
            self.content = self.content.cleared();
        }
    }

    /// Feeds the content what arrived for its instrument this frame.
    pub fn on_frame(&mut self, feed: &Feed, trades: &[protocol::Trade], now_ms: u64) {
        let Some(id) = self.instrument.as_deref() else {
            return;
        };
        let (Some(market), Some(instrument)) = (feed.market(id), feed.instrument(id)) else {
            return;
        };
        let own: Vec<model::Trade> = trades
            .iter()
            .filter(|trade| trade.instrument == id)
            .map(model::Trade::from)
            .collect();

        match &mut self.content {
            Content::Starter => {}
            Content::Ladder(ladder) => ladder.update(
                min_tick(instrument.coin.price_decimals()),
                &market.book,
                market.book_ts,
                &own,
            ),
            Content::TimeAndSales(tape) => tape.insert_buffer(&own, now_ms),
        }
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

        let mut top_left = row![
            link_group_button(self.link_group, show(Modal::LinkGroup)),
            instrument_button,
        ]
        .padding(padding::left(4))
        .align_y(Alignment::Center)
        .spacing(8)
        .height(Length::Fixed(32.0));

        if let Content::Ladder(ladder) = &self.content {
            let label = ladder
                .step()
                .map_or_else(|| ladder.multiplier().to_string(), |step| step.to_string());
            top_left = top_left.push(
                button(
                    text(label)
                        .size(style::text_size::BODY)
                        .align_y(Alignment::Center),
                )
                .on_press(show(Modal::TickSize))
                .style(|theme, status| {
                    style::button::modifier(theme, status, self.modal != Some(Modal::TickSize))
                })
                .height(PANE_CONTROL_BTN_HEIGHT),
            );
        }

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

        let body = self.with_modal(pane, total, maximized, self.body(pane, feed), feed);

        pane_grid::Content::new(body)
            .style(move |theme| style::pane_background(theme, is_focused))
            .title_bar(if self.modal.is_none() {
                title_bar
            } else {
                title_bar.always_show_controls()
            })
    }

    fn body<'a>(&'a self, pane: pane_grid::Pane, feed: &'a Feed) -> Element<'a, Message> {
        let chart = |view: Element<'a, chart::Message>, is_empty: bool| {
            self.chart_body(feed, is_empty, view.map(move |m| Message::Chart(pane, m)))
        };
        let decimals = self
            .instrument
            .as_deref()
            .and_then(|id| feed.instrument(id))
            .map_or(u8::MAX, |i| i.coin.price_decimals());

        match &self.content {
            Content::Starter => starter(pane, feed),
            Content::Ladder(ladder) => {
                chart(chart::ladder::view(ladder, decimals), ladder.is_empty())
            }
            Content::TimeAndSales(tape) => {
                chart(chart::timeandsales::view(tape, decimals), tape.is_empty())
            }
        }
    }

    /// The chart once it has data, or why it has none.
    fn chart_body<'a>(
        &self,
        feed: &Feed,
        is_empty: bool,
        view: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let title = |content: String| -> Element<'a, Message> {
            center(text(content).size(style::text_size::TITLE)).into()
        };
        match self.instrument.as_deref() {
            None => center(
                column![
                    text(self.content.kind().to_string()).size(style::text_size::TITLE),
                    text("No instrument selected").size(style::text_size::SECTION),
                ]
                .spacing(8)
                .align_x(Alignment::Center),
            )
            .into(),
            Some(id) if matches!(feed.status(), Status::Connected { .. }) && !feed.offers(id) => {
                title(format!("The server doesn't offer {id}"))
            }
            Some(_) if is_empty => title("Waiting for data...".to_owned()),
            Some(_) => container(view)
                .padding(padding::left(1).right(1).bottom(1))
                .into(),
        }
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

        let mut buttons = row![];
        if !matches!(self.content, Content::Starter) {
            buttons = buttons.push(button_with_tooltip(
                icon_text(Icon::Cog, 12),
                Message::ShowModal(pane, Modal::Settings),
                None,
                Position::Bottom,
                modal_style(Modal::Settings),
            ));
        }

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
            Some(Modal::TickSize) => match &self.content {
                Content::Ladder(ladder) => stack_modal(
                    base,
                    settings::tick_size_view(pane, ladder.multiplier()),
                    on_blur,
                    padding::left(12),
                    Alignment::Start,
                ),
                _ => base,
            },
            Some(Modal::Settings) => {
                let content = match &self.content {
                    Content::Starter => return base,
                    Content::Ladder(ladder) => settings::ladder_cfg_view(ladder.config, pane),
                    Content::TimeAndSales(tape) => {
                        settings::timesales_cfg_view(tape.config(), pane)
                    }
                };
                stack_modal(
                    base,
                    content,
                    on_blur,
                    padding::right(12).left(12),
                    Alignment::End,
                )
            }
            Some(Modal::Controls) => stack_modal(
                base,
                container(self.controls(pane, total, maximized)).style(style::chart_modal),
                on_blur,
                padding::left(12),
                Alignment::End,
            ),
        }
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

/// A pane that isn't a view yet: the views to pick from, and the connection.
fn starter(pane: pane_grid::Pane, feed: &Feed) -> Element<'_, Message> {
    let status = match feed.status() {
        Status::Connected { session } => text(format!("Connected, session {session}")),
        Status::Connecting { last_error: None } => text("Connecting..."),
        Status::Connecting {
            last_error: Some(error),
        } => text(format!("Connecting... ({error})")),
    }
    .size(style::text_size::BODY)
    .style(style::secondary_text);

    let views = pick_list(ContentKind::VIEWS, None::<ContentKind>, move |kind| {
        Message::SelectContent(pane, kind)
    })
    .placeholder("Choose a view");

    center(
        column![
            text("Choose a view to get started").size(style::text_size::TITLE),
            views,
            status,
        ]
        .align_x(Alignment::Center)
        .spacing(12),
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::charts::model::TickMultiplier;

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

    #[test]
    fn each_view_asks_for_what_it_draws() {
        assert_eq!(
            Content::new(ContentKind::Starter).streams(),
            Streams::default()
        );
        assert_eq!(Content::new(ContentKind::Ladder).streams(), Streams::ALL);
        assert_eq!(
            Content::new(ContentKind::TimeAndSales).streams(),
            Streams {
                trades: true,
                books: false
            }
        );
    }

    #[test]
    fn a_new_instrument_starts_the_view_over_with_its_settings() {
        let mut state = State::new(ContentKind::Ladder);
        let Content::Ladder(ladder) = &mut state.content else {
            unreachable!()
        };
        ladder.set_multiplier(TickMultiplier(10));

        state.set_instrument("A");
        let Content::Ladder(ladder) = &state.content else {
            unreachable!()
        };
        assert_eq!(ladder.multiplier(), TickMultiplier(10));
        assert_eq!(state.instrument.as_deref(), Some("A"));
    }
}
