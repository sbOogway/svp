// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (src/screen/dashboard.rs, data/src/layout/pane.rs @ ab8f3c1), GPL-3.0-or-later,
// by the flowsurface contributors.

//! The dashboard: a grid of panes the user splits, drags, resizes and
//! closes, each on an instrument, and linked in groups that switch
//! instrument together.

mod settings;
mod state;

use std::collections::BTreeMap;

use iced::{
    Element, Task,
    widget::{
        PaneGrid,
        pane_grid::{self, Axis, Configuration, DragEvent, Pane, ResizeEvent},
    },
};
pub use state::{Content, ContentKind, Modal, State};
use svp_common::protocol;

use super::{
    chart,
    feed::{Feed, Streams},
    model::{TickMultiplier, ladder, timeandsales},
    ui::style,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkGroup {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
}

impl LinkGroup {
    pub const ALL: [LinkGroup; 9] = [
        LinkGroup::A,
        LinkGroup::B,
        LinkGroup::C,
        LinkGroup::D,
        LinkGroup::E,
        LinkGroup::F,
        LinkGroup::G,
        LinkGroup::H,
        LinkGroup::I,
    ];
}

impl std::fmt::Display for LinkGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = LinkGroup::ALL.iter().position(|g| g == self).unwrap_or(0) + 1;
        write!(f, "{n}")
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Clicked(Pane),
    Resized(ResizeEvent),
    Dragged(DragEvent),
    Split(Axis, Pane),
    Close(Pane),
    Maximize(Pane),
    Restore,
    Reset(Pane),
    SwitchLinkGroup(Pane, Option<LinkGroup>),
    SelectInstrument(Pane, String),
    SelectContent(Pane, ContentKind),
    SetTickMultiplier(Pane, TickMultiplier),
    SetLadderConfig(Pane, ladder::Config),
    SetTapeConfig(Pane, timeandsales::Config),
    Chart(Pane, chart::Message),
    ShowModal(Pane, Modal),
    HideModal(Pane),
    Search(Pane, String),
}

pub struct Dashboard {
    panes: pane_grid::State<State>,
    focus: Option<Pane>,
    /// UNIX milliseconds of the last frame, for what scrolling prunes.
    now_ms: u64,
}

impl Default for Dashboard {
    /// Charts to pick on the left, a tape below them and the ladder on the right.
    fn default() -> Self {
        let pane = |kind| Box::new(Configuration::Pane(State::new(kind)));
        let split = |axis, ratio, a, b| Box::new(Configuration::Split { axis, ratio, a, b });
        Self {
            panes: pane_grid::State::with_configuration(*split(
                Axis::Vertical,
                0.8,
                split(
                    Axis::Horizontal,
                    0.4,
                    split(
                        Axis::Vertical,
                        0.5,
                        pane(ContentKind::Starter),
                        pane(ContentKind::Starter),
                    ),
                    split(
                        Axis::Vertical,
                        0.5,
                        pane(ContentKind::Starter),
                        pane(ContentKind::TimeAndSales),
                    ),
                ),
                pane(ContentKind::Ladder),
            )),
            focus: None,
            now_ms: 0,
        }
    }
}

impl Dashboard {
    pub fn focus(&self) -> Option<Pane> {
        self.focus
    }

    pub fn panes(&self) -> impl Iterator<Item = (Pane, &State)> {
        self.panes.iter().map(|(&pane, state)| (pane, state))
    }

    pub fn get(&self, pane: Pane) -> Option<&State> {
        self.panes.get(pane)
    }

    /// What every pane needs, merged per instrument.
    pub fn wanted(&self) -> BTreeMap<String, Streams> {
        Feed::merge(self.panes.iter().filter_map(|(_, state)| {
            let streams = state.content.streams();
            let instrument = state.instrument.as_deref()?;
            (streams != Streams::default()).then_some((instrument, streams))
        }))
    }

    /// Puts every pane on `instrument` when none has picked one, so a
    /// fresh dashboard shows data as soon as the server offers some.
    pub fn seed(&mut self, instrument: &str) {
        if self
            .panes
            .iter()
            .all(|(_, state)| state.instrument.is_none())
        {
            for (_, state) in self.panes.iter_mut() {
                state.set_instrument(instrument);
            }
        }
    }

    /// Hands each pane what arrived for its instrument this frame.
    pub fn on_frame(&mut self, feed: &Feed, trades: &[protocol::Trade], now_ms: u64) {
        self.now_ms = now_ms;
        for (_, state) in self.panes.iter_mut() {
            state.on_frame(feed, trades, now_ms);
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Clicked(pane) => self.focus = Some(pane),
            Message::Resized(ResizeEvent { split, ratio }) => self.panes.resize(split, ratio),
            Message::Dragged(DragEvent::Dropped { pane, target }) => self.panes.drop(pane, target),
            Message::Dragged(_) => {}
            Message::Split(axis, pane) => {
                if let Some((new, _)) = self.panes.split(axis, pane, State::default()) {
                    self.focus = Some(new);
                }
            }
            Message::Close(pane) => {
                if let Some((_, sibling)) = self.panes.close(pane) {
                    self.focus = Some(sibling);
                }
            }
            Message::Maximize(pane) => self.panes.maximize(pane),
            Message::Restore => self.panes.restore(),
            Message::Reset(pane) => {
                if let Some(state) = self.panes.get_mut(pane) {
                    *state = State::default();
                }
            }
            Message::SwitchLinkGroup(pane, group) => self.switch_link_group(pane, group),
            Message::SelectInstrument(pane, instrument) => self.select(pane, &instrument),
            Message::SelectContent(pane, kind) => {
                if let Some(state) = self.panes.get_mut(pane) {
                    state.content = Content::new(kind);
                    state.modal = None;
                }
            }
            Message::SetTickMultiplier(pane, multiplier) => {
                if let Some(state) = self.panes.get_mut(pane)
                    && let Content::Ladder(ladder) = &mut state.content
                {
                    ladder.set_multiplier(multiplier);
                    state.modal = None;
                }
            }
            Message::SetLadderConfig(pane, config) => {
                if let Some(State {
                    content: Content::Ladder(ladder),
                    ..
                }) = self.panes.get_mut(pane)
                {
                    ladder.set_config(config);
                }
            }
            Message::SetTapeConfig(pane, config) => {
                if let Some(State {
                    content: Content::TimeAndSales(tape),
                    ..
                }) = self.panes.get_mut(pane)
                {
                    tape.set_config(config);
                }
            }
            Message::Chart(pane, message) => {
                if let Some(state) = self.panes.get_mut(pane) {
                    Self::chart(&mut state.content, message, self.now_ms);
                }
            }
            Message::ShowModal(pane, modal) => {
                if let Some(search) = self
                    .panes
                    .get_mut(pane)
                    .and_then(|state| state.show_modal(modal))
                {
                    return iced::widget::operation::focus(search);
                }
            }
            Message::HideModal(pane) => {
                if let Some(state) = self.panes.get_mut(pane) {
                    state.modal = None;
                }
            }
            Message::Search(pane, query) => {
                if let Some(state) = self.panes.get_mut(pane) {
                    state.query = query;
                }
            }
        }
        Task::none()
    }

    fn chart(content: &mut Content, message: chart::Message, now_ms: u64) {
        match (content, message) {
            (Content::Ladder(ladder), chart::Message::Scrolled(delta)) => ladder.scroll(delta),
            (Content::Ladder(ladder), chart::Message::ResetScroll) => ladder.reset_scroll(),
            (Content::TimeAndSales(tape), chart::Message::Scrolled(delta)) => {
                tape.scroll(delta, now_ms);
            }
            (Content::TimeAndSales(tape), chart::Message::ResetScroll) => {
                tape.reset_scroll(now_ms);
            }
            (Content::Starter, _) => {}
        }
    }

    /// Puts `pane` on `instrument`, and every pane linked to it.
    pub fn select(&mut self, pane: Pane, instrument: &str) {
        let Some(state) = self.panes.get_mut(pane) else {
            return;
        };
        state.modal = None;
        state.query.clear();
        let group = state.link_group;
        for (&other, state) in self.panes.iter_mut() {
            if other == pane || group.is_some_and(|g| state.link_group == Some(g)) {
                state.set_instrument(instrument);
            }
        }
    }

    /// Joining a group takes the instrument the group is on.
    fn switch_link_group(&mut self, pane: Pane, group: Option<LinkGroup>) {
        let joined = group.and_then(|group| {
            self.panes
                .iter()
                .filter(|&(&other, state)| other != pane && state.link_group == Some(group))
                .find_map(|(_, state)| state.instrument.clone())
        });
        if let Some(state) = self.panes.get_mut(pane) {
            state.link_group = group;
            state.modal = None;
            if let Some(joined) = joined {
                state.set_instrument(&joined);
            }
        }
    }

    /// Closes the focused pane's modal; `false` when there was none.
    pub fn go_back(&mut self) -> bool {
        self.focus
            .and_then(|pane| self.panes.get_mut(pane))
            .is_some_and(|state| state.modal.take().is_some())
    }

    pub fn view<'a>(&'a self, feed: &'a Feed) -> Element<'a, Message> {
        let total = self.panes.len();
        PaneGrid::new(&self.panes, |pane, state, maximized| {
            state.view(pane, total, self.focus == Some(pane), maximized, feed)
        })
        .min_size(240)
        .on_click(Message::Clicked)
        .on_drag(Message::Dragged)
        .on_resize(8, Message::Resized)
        .spacing(6)
        .style(style::pane_grid)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dashboard() -> (Dashboard, Vec<Pane>) {
        let dashboard = Dashboard::default();
        let mut panes: Vec<Pane> = dashboard.panes().map(|(pane, _)| pane).collect();
        panes.sort_by_key(|pane| format!("{pane:?}"));
        (dashboard, panes)
    }

    fn instrument(dashboard: &Dashboard, pane: Pane) -> Option<&str> {
        dashboard.get(pane).unwrap().instrument.as_deref()
    }

    #[test]
    fn the_default_dashboard_has_five_empty_panes_until_seeded() {
        let (mut dashboard, panes) = dashboard();
        assert_eq!(panes.len(), 5);
        assert!(dashboard.wanted().is_empty());

        dashboard.seed("A");
        assert_eq!(
            dashboard.wanted(),
            BTreeMap::from([("A".to_owned(), Streams::ALL)])
        );

        let _ = dashboard.update(Message::SelectInstrument(panes[0], "B".into()));
        dashboard.seed("C");
        assert_eq!(instrument(&dashboard, panes[0]), Some("B"));
        assert_eq!(instrument(&dashboard, panes[1]), Some("A"));
    }

    #[test]
    fn split_focuses_the_new_pane_and_close_its_sibling() {
        let (mut dashboard, panes) = dashboard();
        let _ = dashboard.update(Message::Split(Axis::Horizontal, panes[0]));
        let new = dashboard.focus().unwrap();
        assert!(!panes.contains(&new));
        assert_eq!(dashboard.panes().count(), 6);

        let _ = dashboard.update(Message::Close(new));
        assert_eq!(dashboard.panes().count(), 5);
        assert_eq!(dashboard.focus(), Some(panes[0]));
    }

    #[test]
    fn linked_panes_switch_instrument_together() {
        let (mut dashboard, panes) = dashboard();
        let [a, b, c, ..] = panes[..] else {
            unreachable!()
        };
        let _ = dashboard.update(Message::SelectInstrument(a, "A".into()));
        let _ = dashboard.update(Message::SwitchLinkGroup(a, Some(LinkGroup::A)));
        let _ = dashboard.update(Message::SwitchLinkGroup(b, Some(LinkGroup::A)));
        assert_eq!(
            instrument(&dashboard, b),
            Some("A"),
            "joining takes the group's"
        );

        let _ = dashboard.update(Message::SelectInstrument(b, "B".into()));
        assert_eq!(instrument(&dashboard, a), Some("B"));
        assert_eq!(instrument(&dashboard, c), None, "unlinked panes stay");

        let _ = dashboard.update(Message::SwitchLinkGroup(b, None));
        let _ = dashboard.update(Message::SelectInstrument(b, "C".into()));
        assert_eq!(instrument(&dashboard, a), Some("B"));
    }

    fn kinds(dashboard: &Dashboard) -> Vec<ContentKind> {
        let mut kinds: Vec<_> = dashboard
            .panes()
            .map(|(_, state)| state.content.kind())
            .collect();
        kinds.sort_by_key(ToString::to_string);
        kinds
    }

    fn ladder(dashboard: &Dashboard, pane: Pane) -> &ladder::Ladder {
        match &dashboard.get(pane).unwrap().content {
            Content::Ladder(ladder) => ladder,
            other => panic!("not a ladder: {other:?}"),
        }
    }

    fn tape(dashboard: &Dashboard, pane: Pane) -> &timeandsales::TimeAndSales {
        match &dashboard.get(pane).unwrap().content {
            Content::TimeAndSales(tape) => tape,
            other => panic!("not a tape: {other:?}"),
        }
    }

    fn pane_of(dashboard: &Dashboard, kind: ContentKind) -> Pane {
        dashboard
            .panes()
            .find(|(_, state)| state.content.kind() == kind)
            .unwrap()
            .0
    }

    #[test]
    fn the_default_dashboard_has_a_ladder_a_tape_and_starters() {
        let (dashboard, _) = dashboard();
        assert_eq!(
            kinds(&dashboard),
            [
                ContentKind::Ladder,
                ContentKind::Starter,
                ContentKind::Starter,
                ContentKind::Starter,
                ContentKind::TimeAndSales,
            ]
        );
    }

    #[test]
    fn panes_on_one_instrument_merge_their_streams() {
        let (mut dashboard, panes) = dashboard();
        for &pane in &panes {
            let _ = dashboard.update(Message::Reset(pane));
        }
        for &pane in &panes[..2] {
            let _ = dashboard.update(Message::SelectInstrument(pane, "A".into()));
        }
        assert!(dashboard.wanted().is_empty(), "starters want nothing");

        let trades = Streams {
            trades: true,
            books: false,
        };
        let _ = dashboard.update(Message::SelectContent(panes[0], ContentKind::TimeAndSales));
        assert_eq!(
            dashboard.wanted(),
            BTreeMap::from([("A".to_owned(), trades)])
        );

        let _ = dashboard.update(Message::SelectContent(panes[1], ContentKind::Ladder));
        assert_eq!(
            dashboard.wanted(),
            BTreeMap::from([("A".to_owned(), Streams::ALL)])
        );

        let _ = dashboard.update(Message::Reset(panes[1]));
        assert_eq!(
            dashboard.wanted(),
            BTreeMap::from([("A".to_owned(), trades)])
        );
    }

    #[test]
    fn the_ladder_takes_its_tick_size_and_settings() {
        let (mut dashboard, _) = dashboard();
        let pane = pane_of(&dashboard, ContentKind::Ladder);
        let _ = dashboard.update(Message::ShowModal(pane, Modal::TickSize));
        let _ = dashboard.update(Message::SetTickMultiplier(pane, TickMultiplier(25)));
        assert_eq!(ladder(&dashboard, pane).multiplier(), TickMultiplier(25));
        assert_eq!(dashboard.get(pane).unwrap().modal, None);

        let config = ladder::Config {
            show_spread: true,
            ..ladder::Config::default()
        };
        let _ = dashboard.update(Message::SetLadderConfig(pane, config));
        assert_eq!(ladder(&dashboard, pane).config, config);

        let _ = dashboard.update(Message::SelectInstrument(pane, "B".into()));
        assert_eq!(
            ladder(&dashboard, pane).config,
            config,
            "kept across instruments"
        );

        let tape_pane = pane_of(&dashboard, ContentKind::TimeAndSales);
        let _ = dashboard.update(Message::SetLadderConfig(tape_pane, config));
        let _ = dashboard.update(Message::SetTickMultiplier(tape_pane, TickMultiplier(5)));
        assert_eq!(
            tape(&dashboard, tape_pane).config(),
            timeandsales::Config::default(),
            "a ladder's settings don't apply to a tape"
        );
    }

    #[test]
    fn the_tape_takes_its_settings() {
        let (mut dashboard, _) = dashboard();
        let pane = pane_of(&dashboard, ContentKind::TimeAndSales);
        let config = timeandsales::Config {
            trade_size_filter: 1_000.0,
            stacked_bar: Some(timeandsales::StackedBar::Full(
                timeandsales::StackedBarRatio::Count,
            )),
            ..timeandsales::Config::default()
        };
        let _ = dashboard.update(Message::SetTapeConfig(pane, config));
        assert_eq!(tape(&dashboard, pane).config(), config);

        let _ = dashboard.update(Message::SelectContent(pane, ContentKind::Ladder));
        let _ = dashboard.update(Message::SelectContent(pane, ContentKind::TimeAndSales));
        assert_eq!(
            tape(&dashboard, pane).config(),
            timeandsales::Config::default(),
            "a new view starts from the defaults"
        );
    }

    #[test]
    fn frames_feed_the_ladder_the_book_and_the_tape_the_trades() {
        use svp_common::{
            market::Coin,
            protocol::{
                BookData, BookUpdate, Instrument, Market, Message as Wire, Price, PriceStep,
                Quantity, Side,
            },
        };

        use crate::charts::feed::{Commands, Event};

        fn px(s: &str) -> Price {
            s.parse().unwrap()
        }
        fn qty(s: &str) -> Quantity {
            s.parse().unwrap()
        }
        let (commands, _receiver) = Commands::new();
        let mut feed = Feed::default();
        feed.apply(Event::Connected {
            session: 1,
            instruments: vec![Instrument {
                id: "A".into(),
                coin: Coin::BTC,
                market: Market::Perp,
                venues: vec![],
            }],
            commands,
        });

        let (mut dashboard, _) = dashboard();
        dashboard.seed("A");
        feed.sync(&dashboard.wanted());
        feed.apply(Event::Received(Wire::Book(BookUpdate {
            instrument: "A".into(),
            ts: 5,
            data: BookData::Snapshot {
                bids: vec![(px("100.01"), qty("1"))],
                asks: vec![(px("100.02"), qty("2"))],
            },
        })));
        for (instrument, price) in [("A", "100.02"), ("B", "5")] {
            feed.apply(Event::Received(Wire::Trade(protocol::Trade {
                instrument: instrument.into(),
                ts: 7_000_000,
                price: px(price),
                size: qty("0.5"),
                aggressor: Some(Side::Buy),
                id: "t".into(),
            })));
        }
        let trades = feed.tick(std::time::Instant::now());
        dashboard.on_frame(&feed, &trades, 7);

        let ladder_pane = pane_of(&dashboard, ContentKind::Ladder);
        let ladder = ladder(&dashboard, ladder_pane);
        assert_eq!(ladder.step(), "0.01".parse::<PriceStep>().ok());
        assert_eq!(ladder.best_price(ladder::Side::Bid), Some(px("100.01")));
        assert_eq!(ladder.trade_qty_at(px("100.02")).buy, qty("0.5"));

        let tape_pane = pane_of(&dashboard, ContentKind::TimeAndSales);
        let rows: Vec<_> = tape(&dashboard, tape_pane)
            .visible_trades(100.0)
            .map(|(_, trade)| (trade.time, trade.price))
            .collect();
        assert_eq!(rows, [(7, px("100.02"))], "only A's trades");

        let _ = dashboard.update(Message::Chart(tape_pane, chart::Message::Scrolled(-100.0)));
        let _ = dashboard.update(Message::Chart(tape_pane, chart::Message::ResetScroll));
        assert!(!tape(&dashboard, tape_pane).is_paused());
    }

    #[test]
    fn a_modal_toggles_and_going_back_closes_it() {
        let (mut dashboard, panes) = dashboard();
        let pane = panes[0];
        let _ = dashboard.update(Message::Clicked(pane));
        let _ = dashboard.update(Message::ShowModal(pane, Modal::Settings));
        assert_eq!(dashboard.get(pane).unwrap().modal, Some(Modal::Settings));
        let _ = dashboard.update(Message::ShowModal(pane, Modal::Settings));
        assert_eq!(dashboard.get(pane).unwrap().modal, None);

        let _ = dashboard.update(Message::ShowModal(pane, Modal::LinkGroup));
        assert!(dashboard.go_back());
        assert!(!dashboard.go_back());
    }
}
