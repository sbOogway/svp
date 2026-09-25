//! Lists the instruments the server offers, filtered by a search box.

use iced::{
    Alignment, Element, Length,
    widget::{Id, button, column, container, row, scrollable, space, text, text_input},
};
use svp_common::protocol::{Instrument, Market};

use super::style;

/// Whether `instrument` answers a search for `query`, ignoring case.
pub fn matches(instrument: &Instrument, query: &str) -> bool {
    let query = query.trim().to_uppercase();
    query.is_empty()
        || instrument.id.to_uppercase().contains(&query)
        || instrument.coin.to_uppercase().contains(&query)
}

pub fn view<'a, Message: Clone + 'a>(
    instruments: &'a [Instrument],
    query: &str,
    search_id: Id,
    selected: Option<&str>,
    on_search: impl Fn(String) -> Message + 'a,
    on_select: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    let search = text_input("Search for an instrument...", query)
        .id(search_id)
        .on_input(on_search)
        .padding(6);

    let rows = instruments
        .iter()
        .filter(|instrument| matches(instrument, query))
        .map(|instrument| {
            let is_selected = selected == Some(instrument.id.as_str());
            let market = match instrument.market {
                Market::Spot => "SPOT",
                Market::Perp => "PERP",
            };
            let label = row![
                text(&instrument.id).size(style::text_size::BODY),
                space::horizontal(),
                text(format!("{market} · {} venues", instrument.venues.len()))
                    .size(style::text_size::SMALL)
                    .style(style::secondary_text),
            ]
            .align_y(Alignment::Center)
            .spacing(8);
            button(label)
                .width(Length::Fill)
                .style(move |theme, status| style::button::menu_body(theme, status, is_selected))
                .on_press(on_select(instrument.id.clone()))
                .into()
        })
        .collect::<Vec<Element<'a, Message>>>();

    let list: Element<'a, Message> = if rows.is_empty() {
        container(
            text(if instruments.is_empty() {
                "The server hasn't offered any instrument yet"
            } else {
                "No instrument matches"
            })
            .style(style::secondary_text),
        )
        .padding(8)
        .into()
    } else {
        scrollable(column(rows).spacing(4).padding(iced::padding::right(8))).into()
    };

    column![search, list].spacing(8).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_matches_the_id_or_the_coin_in_any_case() {
        let btc = Instrument {
            id: "BTC-PERP.SVP".into(),
            coin: "BTC".into(),
            market: Market::Perp,
            venues: vec![],
        };
        assert!(matches(&btc, ""));
        assert!(matches(&btc, " perp "));
        assert!(matches(&btc, "btc"));
        assert!(!matches(&btc, "eth"));
    }
}
