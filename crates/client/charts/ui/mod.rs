// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (src/widget.rs, src/modal.rs, src/modal/pane.rs @ ab8f3c1), GPL-3.0-or-later,
// by the flowsurface contributors.

//! Widgets and styles the dashboard shares.

pub mod picker;
pub mod sidebar;
pub mod style;

use iced::{
    Alignment, Color, Element, Length, Theme, border, padding,
    widget::{
        Column, button, column, container, mouse_area, opaque, row, rule, slider, space, stack,
        text, tooltip::Position,
    },
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

/// `items` one under the other, with a thin rule between each.
pub fn split_column<'a, Message: 'a>(
    items: impl IntoIterator<Item = Element<'a, Message>>,
) -> Column<'a, Message> {
    let mut col = column![];
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            col = col.push(rule::horizontal(1.0).style(style::split_ruler));
        }
        col = col.push(item);
    }
    col
}

pub fn classic_slider_row<'a, Message: Clone + 'a>(
    label: iced::widget::Text<'a>,
    slider: Element<'a, Message>,
    placeholder: Option<iced::widget::Text<'a>>,
) -> Element<'a, Message> {
    let slider = if let Some(placeholder) = placeholder {
        column![slider, placeholder]
            .spacing(2)
            .align_x(Alignment::Center)
    } else {
        column![slider]
    };

    container(
        row![label, slider]
            .align_y(Alignment::Center)
            .spacing(8)
            .padding(8),
    )
    .style(style::modal_container)
    .into()
}

/// A slider as a bar, with `label` and the value written over it.
pub fn labeled_slider<'a, Message: Clone + 'a>(
    label: &'a str,
    range: std::ops::RangeInclusive<f32>,
    current: f32,
    on_change: impl Fn(f32) -> Message + 'a,
    to_string: impl Fn(f32) -> String,
    step: f32,
) -> Element<'a, Message> {
    let slider = slider(range, current, on_change)
        .step(step)
        .width(Length::Fill)
        .height(24)
        .style(|theme: &Theme, status| {
            let palette = theme.extended_palette();

            slider::Style {
                rail: slider::Rail {
                    backgrounds: (
                        palette.background.strong.color.into(),
                        Color::TRANSPARENT.into(),
                    ),
                    width: 24.0,
                    border: border::rounded(2),
                },
                handle: slider::Handle {
                    shape: slider::HandleShape::Rectangle {
                        width: 2,
                        border_radius: 2.0.into(),
                    },
                    background: match status {
                        slider::Status::Active => palette.background.strong.color.into(),
                        slider::Status::Hovered => palette.primary.base.color.into(),
                        slider::Status::Dragged => palette.primary.weak.color.into(),
                    },
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                },
            }
        });

    stack![
        container(slider).style(style::modal_container),
        row![text(label), space::horizontal(), text(to_string(current))]
            .padding([0, 10])
            .height(Length::Fill)
            .align_y(Alignment::Center),
    ]
    .into()
}
