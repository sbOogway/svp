// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (src/screen/dashboard.rs, data/src/layout/pane.rs @ ab8f3c1), GPL-3.0-or-later,
// by the flowsurface contributors.

//! The dashboard: a grid of panes the user splits, drags, resizes and
//! closes, each on an instrument, and linked in groups that switch
//! instrument together.

mod state;

use std::collections::BTreeMap;

use iced::{
    Element, Task,
    widget::{
        PaneGrid,
        pane_grid::{self, Axis, Configuration, DragEvent, Pane, ResizeEvent},
    },
};
pub use state::{Modal, State};

use super::{
    feed::{Feed, Streams},
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
    SetStreams(Pane, Streams),
    ShowModal(Pane, Modal),
    HideModal(Pane),
    Search(Pane, String),
}

pub struct Dashboard {
    panes: pane_grid::State<State>,
    focus: Option<Pane>,
}

impl Default for Dashboard {
    fn default() -> Self {
        let pane = || Box::new(Configuration::Pane(State::default()));
        let split = |axis, ratio, a, b| Box::new(Configuration::Split { axis, ratio, a, b });
        Self {
            panes: pane_grid::State::with_configuration(*split(
                Axis::Vertical,
                0.8,
                split(
                    Axis::Horizontal,
                    0.4,
                    split(Axis::Vertical, 0.5, pane(), pane()),
                    split(Axis::Vertical, 0.5, pane(), pane()),
                ),
                pane(),
            )),
            focus: None,
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
            state
                .instrument
                .as_deref()
                .map(|instrument| (instrument, state.streams))
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
                state.instrument = Some(instrument.to_owned());
            }
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
            Message::SetStreams(pane, streams) => {
                if let Some(state) = self.panes.get_mut(pane) {
                    state.streams = streams;
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
                state.instrument = Some(instrument.to_owned());
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
            if joined.is_some() {
                state.instrument = joined;
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

    #[test]
    fn panes_on_one_instrument_merge_their_streams() {
        let (mut dashboard, panes) = dashboard();
        let trades = Streams {
            trades: true,
            books: false,
        };
        let books = Streams {
            trades: false,
            books: true,
        };
        for (&pane, streams) in panes.iter().zip([trades, books]) {
            let _ = dashboard.update(Message::SelectInstrument(pane, "A".into()));
            let _ = dashboard.update(Message::SetStreams(pane, streams));
        }
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
