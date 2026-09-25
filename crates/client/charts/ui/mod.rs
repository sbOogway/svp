// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (src/widget.rs, src/modal.rs, src/modal/pane.rs @ ab8f3c1), GPL-3.0-or-later,
// by the flowsurface contributors.

//! Widgets and styles the dashboard shares.

pub mod picker;
pub mod sidebar;
pub mod style;

use iced::{
    Alignment, Element, Length, Theme, padding,
    widget::{button, container, mouse_area, opaque, stack, text, tooltip::Position},
};

use super::pane::LinkGroup;

pub const PANE_CONTROL_BTN_HEIGHT: f32 = 26.0;

pub fn tooltip<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tooltip: Option<&'a str>,
    position: Position,
) -> Element<'a, Message> {
    match tooltip {
        Some(tooltip) => iced::widget::tooltip(
            content,
            container(text(tooltip)).style(style::tooltip).padding(8),
            position,
        )
        .into(),
        None => content.into(),
    }
}

pub fn button_with_tooltip<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    message: Message,
    tooltip_text: Option<&'a str>,
    tooltip_pos: Position,
    style_fn: impl Fn(&Theme, button::Status) -> button::Style + 'static,
) -> Element<'a, Message> {
    tooltip(
        button(content).style(style_fn).on_press(message),
        tooltip_text,
        tooltip_pos,
    )
}

pub fn link_group_button<'a, Message: Clone + 'a>(
    link_group: Option<LinkGroup>,
    on_press: Message,
) -> Element<'a, Message> {
    let is_active = link_group.is_some();
    let label = link_group.map_or_else(|| "-".to_owned(), |group| group.to_string());

    button(
        text(label)
            .font(style::AZERET_MONO)
            .align_x(Alignment::Start)
            .align_y(Alignment::Center),
    )
    .style(move |theme: &Theme, status| style::button::bordered_toggle(theme, status, is_active))
    .on_press(on_press)
    .height(PANE_CONTROL_BTN_HEIGHT)
    .width(28)
    .into()
}

/// `content` over `base` inside a pane; a click outside it sends `on_blur`.
pub fn stack_modal<'a, Message: Clone + 'a>(
    base: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    on_blur: Message,
    padding: padding::Padding,
    alignment: Alignment,
) -> Element<'a, Message> {
    stack![
        base.into(),
        mouse_area(
            container(opaque(content))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(padding)
                .align_x(alignment)
        )
        .on_press(on_blur)
    ]
    .into()
}

/// `content` over the whole dashboard; a click outside it sends `on_blur`.
pub fn dashboard_modal<'a, Message: Clone + 'a>(
    base: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    on_blur: Message,
    padding: padding::Padding,
    align_y: Alignment,
    align_x: Alignment,
) -> Element<'a, Message> {
    stack![
        base.into(),
        mouse_area(
            container(opaque(content))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(padding)
                .align_y(align_y)
                .align_x(align_x)
        )
        .on_press(on_blur)
    ]
    .into()
}
