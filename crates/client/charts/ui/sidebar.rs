// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (src/screen/dashboard/sidebar.rs @ ab8f3c1), GPL-3.0-or-later,
// by the flowsurface contributors.

use iced::{
    Alignment, Element, Task,
    widget::{Id, column, row, space, tooltip::Position},
};
use svp_common::protocol::Instrument;

use super::{
    button_with_tooltip, picker,
    style::{self, Icon, icon_text},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    Layout,
}

#[derive(Debug, Clone)]
pub enum Message {
    ToggleMenu(Option<Menu>),
    ToggleInstruments,
    Search(String),
    Select(String),
}

pub enum Action {
    /// For the focused pane, or the first one without a focus.
    InstrumentSelected(String),
}

#[derive(Debug)]
pub struct Sidebar {
    menu: Option<Menu>,
    instruments_open: bool,
    query: String,
    search_id: Id,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self {
            menu: None,
            instruments_open: false,
            query: String::new(),
            search_id: Id::unique(),
        }
    }
}

impl Sidebar {
    pub fn menu(&self) -> Option<Menu> {
        self.menu
    }

    pub fn update(&mut self, message: Message) -> (Task<Message>, Option<Action>) {
        match message {
            Message::ToggleMenu(menu) => {
                self.menu = menu.filter(|&m| self.menu != Some(m));
            }
            Message::ToggleInstruments => {
                self.instruments_open = !self.instruments_open;
                if self.instruments_open {
                    return (iced::widget::operation::focus(self.search_id.clone()), None);
                }
            }
            Message::Search(query) => self.query = query,
            Message::Select(id) => return (Task::none(), Some(Action::InstrumentSelected(id))),
        }
        (Task::none(), None)
    }

    /// Closes what is open, innermost first; `false` when nothing was.
    pub fn go_back(&mut self) -> bool {
        if self.menu.take().is_some() {
            return true;
        }
        std::mem::take(&mut self.instruments_open)
    }

    pub fn view<'a>(
        &'a self,
        instruments: &'a [Instrument],
        selected: Option<&str>,
    ) -> Element<'a, Message> {
        let nav_button = |icon, message, is_active| {
            button_with_tooltip(
                icon_text(icon, 14).width(24).align_x(Alignment::Center),
                message,
                None,
                Position::Right,
                move |theme, status| style::button::transparent(theme, status, is_active),
            )
        };

        let nav_buttons = column![
            nav_button(
                Icon::Search,
                Message::ToggleInstruments,
                self.instruments_open
            ),
            nav_button(
                Icon::Layout,
                Message::ToggleMenu(Some(Menu::Layout)),
                self.menu == Some(Menu::Layout)
            ),
            space::vertical(),
        ]
        .width(32)
        .spacing(8);

        if self.instruments_open {
            let table = picker::view(
                instruments,
                &self.query,
                self.search_id.clone(),
                selected,
                Message::Search,
                Message::Select,
            );
            row![nav_buttons, column![table].width(200)]
                .spacing(8)
                .into()
        } else {
            row![nav_buttons].spacing(4).into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_toggles_and_going_back_closes_it_before_the_table() {
        let mut sidebar = Sidebar::default();
        let _ = sidebar.update(Message::ToggleInstruments);
        let _ = sidebar.update(Message::ToggleMenu(Some(Menu::Layout)));
        assert_eq!(sidebar.menu(), Some(Menu::Layout));

        assert!(sidebar.go_back());
        assert_eq!(sidebar.menu(), None);
        assert!(sidebar.instruments_open);
        assert!(sidebar.go_back());
        assert!(!sidebar.go_back());

        let _ = sidebar.update(Message::ToggleMenu(Some(Menu::Layout)));
        let _ = sidebar.update(Message::ToggleMenu(Some(Menu::Layout)));
        assert_eq!(sidebar.menu(), None);
    }

    #[test]
    fn selecting_an_instrument_is_an_action() {
        let mut sidebar = Sidebar::default();
        let (_, action) = sidebar.update(Message::Select("BTC-PERP.SVP".into()));
        assert!(matches!(action, Some(Action::InstrumentSelected(id)) if id == "BTC-PERP.SVP"));
    }
}
