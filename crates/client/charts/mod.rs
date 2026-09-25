//! `svp-app`, a native Iced client of the server laid out like
//! [flowsurface](https://github.com/flowsurface-rs/flowsurface): a
//! sidebar, and a dashboard of panes fed by one connection.

pub mod feed;
pub mod model;
pub mod pane;
pub mod ui;

use std::{borrow::Cow, path::PathBuf, time::Instant};

use iced::{
    Alignment, Element, Length, Subscription, Task, keyboard, padding,
    widget::{button, column, container, row, rule, text, tooltip::Position},
};

use self::{
    feed::Feed,
    pane::Dashboard,
    ui::{
        dashboard_modal,
        sidebar::{self, Menu, Sidebar},
        style, tooltip,
    },
};

pub fn run(socket: PathBuf) -> iced::Result {
    iced::application(move || App::new(socket.clone()), App::update, App::view)
        .title("svp")
        .theme(style::theme())
        .subscription(App::subscription)
        .settings(iced::Settings {
            antialiasing: true,
            fonts: vec![
                Cow::Borrowed(style::AZERET_MONO_BYTES),
                Cow::Borrowed(style::ICONS_BYTES),
            ],
            default_text_size: style::text_size::BODY.into(),
            ..Default::default()
        })
        .run()
}

#[derive(Debug, Clone)]
pub enum Message {
    Feed(feed::Event),
    Dashboard(pane::Message),
    Sidebar(sidebar::Message),
    Tick(Instant),
    GoBack,
}

pub struct App {
    socket: PathBuf,
    feed: Feed,
    dashboard: Dashboard,
    sidebar: Sidebar,
}

impl App {
    pub fn new(socket: PathBuf) -> Self {
        Self {
            socket,
            feed: Feed::default(),
            dashboard: Dashboard::default(),
            sidebar: Sidebar::default(),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = match message {
            Message::Feed(event) => {
                let connected = matches!(event, feed::Event::Connected { .. });
                self.feed.apply(event);
                if !connected {
                    return Task::none();
                }
                if let Some(first) = self.feed.instruments().first() {
                    let first = first.id.clone();
                    self.dashboard.seed(&first);
                }
                Task::none()
            }
            Message::Tick(now) => {
                self.feed.tick(now);
                return Task::none();
            }
            Message::Dashboard(message) => self.dashboard.update(message).map(Message::Dashboard),
            Message::Sidebar(message) => {
                let (task, action) = self.sidebar.update(message);
                if let Some(sidebar::Action::InstrumentSelected(instrument)) = action
                    && let Some(pane) = self.dashboard.focus()
                {
                    self.dashboard.select(pane, &instrument);
                }
                task.map(Message::Sidebar)
            }
            Message::GoBack => {
                let _ = self.sidebar.go_back() || self.dashboard.go_back();
                Task::none()
            }
        };
        self.feed.sync(&self.dashboard.wanted());
        task
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let hotkeys = keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            } => Some(Message::GoBack),
            _ => None,
        });
        Subscription::batch([
            feed::connection(self.socket.clone()).map(Message::Feed),
            iced::window::frames().map(Message::Tick),
            hotkeys,
        ])
    }

    pub fn view(&self) -> Element<'_, Message> {
        let selected = self
            .dashboard
            .focus()
            .and_then(|pane| self.dashboard.get(pane))
            .and_then(|state| state.instrument.as_deref());
        let base = row![
            self.sidebar
                .view(self.feed.instruments(), selected)
                .map(Message::Sidebar),
            self.dashboard.view(&self.feed).map(Message::Dashboard),
        ]
        .spacing(4)
        .padding(8);

        match self.sidebar.menu() {
            None => base.into(),
            Some(Menu::Layout) => dashboard_modal(
                base,
                self.layout_menu(),
                Message::Sidebar(sidebar::Message::ToggleMenu(None)),
                padding::left(44).top(40),
                Alignment::Start,
                Alignment::Start,
            ),
        }
    }

    fn layout_menu(&self) -> Element<'_, Message> {
        let focused = self
            .dashboard
            .focus()
            .and_then(|pane| Some((pane, self.dashboard.get(pane)?)));

        let action = |label, message: Option<pane::Message>| {
            button(text(label).align_x(Alignment::Center))
                .width(Length::Fill)
                .on_press_maybe(message.map(Message::Dashboard))
        };
        let (title, reset, split) = match focused {
            Some((pane, state)) => {
                let group = state
                    .link_group
                    .map_or_else(String::new, |g| format!(" - Group {g}"));
                (
                    format!(
                        "{}{group}",
                        state.instrument.as_deref().unwrap_or("Empty pane")
                    ),
                    Some(pane::Message::Reset(pane)),
                    Some(pane::Message::Split(
                        iced::widget::pane_grid::Axis::Horizontal,
                        pane,
                    )),
                )
            }
            None => ("No pane selected".to_owned(), None, None),
        };
        let has_focus = focused.is_some();

        let content = column![
            text(title),
            row![
                tooltip(
                    action("Reset", reset),
                    has_focus.then_some("Reset selected pane"),
                    Position::Top,
                ),
                tooltip(
                    action("Split", split),
                    has_focus.then_some("Split selected pane horizontally"),
                    Position::Top,
                ),
            ]
            .spacing(8),
            rule::horizontal(1.0),
        ]
        .align_x(Alignment::Center)
        .spacing(20);

        container(content)
            .width(260)
            .padding(24)
            .style(style::dashboard_modal)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use svp_common::{
        market::Coin,
        protocol::{Instrument, Market},
    };
    use tokio::sync::mpsc;

    use super::{
        feed::{Command, Commands, Event, Status, Streams},
        *,
    };

    fn connected(session: u64) -> (Message, mpsc::UnboundedReceiver<Command>) {
        let (commands, receiver) = Commands::new();
        let event = Event::Connected {
            session,
            instruments: vec![Instrument {
                id: "BTC-PERP.SVP".into(),
                coin: Coin::BTC,
                market: Market::Perp,
                venues: vec!["BINANCE".into()],
            }],
            commands,
        };
        (Message::Feed(event), receiver)
    }

    fn disconnected() -> Message {
        Message::Feed(Event::Disconnected {
            reason: "refused".into(),
            retry_in: Duration::from_millis(250),
        })
    }

    fn subscribed_to_btc(receiver: &mut mpsc::UnboundedReceiver<Command>) -> bool {
        receiver.try_recv()
            == Ok(Command::Subscribe(vec![
                svp_common::protocol::Subscription {
                    instrument: "BTC-PERP.SVP".into(),
                    trades: Streams::ALL.trades,
                    books: Streams::ALL.books,
                },
            ]))
    }

    #[test]
    fn the_app_subscribes_on_connect_and_again_after_a_reconnect() {
        let mut app = App::new("svp.sock".into());
        assert_eq!(app.feed.status(), &Status::Connecting { last_error: None });

        let _ = app.update(disconnected());
        assert_eq!(
            app.feed.status(),
            &Status::Connecting {
                last_error: Some("refused".into())
            }
        );

        let (message, mut commands) = connected(1);
        let _ = app.update(message);
        assert_eq!(app.feed.status(), &Status::Connected { session: 1 });
        assert!(subscribed_to_btc(&mut commands));
        assert!(
            app.dashboard
                .panes()
                .all(|(_, state)| state.instrument.as_deref() == Some("BTC-PERP.SVP"))
        );

        let _ = app.update(disconnected());
        let (message, mut commands) = connected(2);
        let _ = app.update(message);
        assert_eq!(app.feed.status(), &Status::Connected { session: 2 });
        assert!(subscribed_to_btc(&mut commands));
    }

    #[test]
    fn changing_instrument_moves_the_subscription() {
        let mut app = App::new("svp.sock".into());
        let (message, mut commands) = connected(1);
        let _ = app.update(message);
        assert!(subscribed_to_btc(&mut commands));

        let panes: Vec<_> = app.dashboard.panes().map(|(pane, _)| pane).collect();
        for &pane in &panes {
            let _ = app.update(Message::Dashboard(pane::Message::SelectInstrument(
                pane,
                "ETH-PERP.SVP".into(),
            )));
        }
        assert!(matches!(commands.try_recv(), Ok(Command::Unsubscribe(_))));
        assert!(commands.try_recv().is_err(), "ETH isn't offered");

        let _ = app.update(Message::Dashboard(pane::Message::Clicked(panes[0])));
        let _ = app.update(Message::Sidebar(sidebar::Message::Select(
            "BTC-PERP.SVP".into(),
        )));
        assert_eq!(
            app.dashboard.get(panes[0]).unwrap().instrument.as_deref(),
            Some("BTC-PERP.SVP")
        );
        assert!(subscribed_to_btc(&mut commands));
    }
}
