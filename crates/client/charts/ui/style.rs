// Adapted from flowsurface, https://github.com/flowsurface-rs/flowsurface
// (src/style.rs, data/src/config/theme.rs @ ab8f3c1), GPL-3.0-or-later,
// by the flowsurface contributors. The palette is svp's own, light.

use iced::{
    Border, Color, Font, Renderer, Shadow, Theme,
    font::{Family, Stretch, Weight},
    theme::Palette,
    widget::{
        self, Text,
        container::Style,
        pane_grid::{Highlight, Line},
    },
};

pub const ICONS_BYTES: &[u8] = include_bytes!("../../assets/fonts/icons.ttf");
pub const ICONS_FONT: Font = Font::with_name("icons");

pub const AZERET_MONO_BYTES: &[u8] = include_bytes!("../../assets/fonts/AzeretMono-Regular.ttf");
pub const AZERET_MONO: Font = Font {
    family: Family::Name("Azeret Mono"),
    weight: Weight::Normal,
    stretch: Stretch::Normal,
    style: iced::font::Style::Normal,
};

pub mod text_size {
    pub const SMALL: f32 = 11.0;
    pub const BODY: f32 = 12.0;
    pub const SECTION: f32 = 14.0;
    pub const TITLE: f32 = 16.0;

    pub const EMPHASIS: f32 = BODY + 1.0;
}

pub fn theme() -> Theme {
    Theme::custom(
        "svp".to_owned(),
        Palette {
            background: Color::WHITE,
            text: Color::from_rgb8(28, 28, 30),
            primary: Color::from_rgb8(90, 90, 96),
            success: Color::from_rgb8(22, 150, 105),
            danger: Color::from_rgb8(192, 80, 77),
            warning: Color::from_rgb8(238, 216, 139),
        },
    )
}

/// Code points of glyphs in `assets/fonts/icons.ttf`.
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum Icon {
    ResizeFull = 59395,
    ResizeSmall = 59396,
    Close = 59397,
    Layout = 59398,
    Cog = 59408,
    Search = 59394,
}

impl From<Icon> for char {
    fn from(icon: Icon) -> Self {
        char::from_u32(icon as u32).expect("icon codepoints must be valid Unicode scalar values")
    }
}

pub fn icon_text<'a>(icon: Icon, size: u16) -> Text<'a, Theme, Renderer> {
    iced::widget::text(char::from(icon).to_string())
        .font(ICONS_FONT)
        .size(iced::Pixels(size.into()))
}

pub fn secondary_text(theme: &Theme) -> widget::text::Style {
    widget::text::Style {
        color: Some(theme.extended_palette().secondary.weak.color),
    }
}

pub fn tooltip(theme: &Theme) -> Style {
    let palette = theme.extended_palette();

    Style {
        background: Some(palette.background.weakest.color.into()),
        border: Border {
            width: 1.0,
            color: palette.background.weak.color,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

pub mod button {
    use iced::{
        Border, Theme,
        widget::button::{Status, Style},
    };

    pub fn transparent(theme: &Theme, status: Status, is_clicked: bool) -> Style {
        let palette = theme.extended_palette();

        Style {
            text_color: palette.background.base.text,
            border: Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            background: match status {
                Status::Active => is_clicked.then(|| palette.background.weak.color.into()),
                Status::Pressed => Some(palette.background.weak.color.into()),
                Status::Hovered => Some(palette.background.strong.color.into()),
                Status::Disabled => {
                    if is_clicked {
                        Some(palette.background.strongest.color.into())
                    } else {
                        Some(palette.background.strong.color.into())
                    }
                }
            },
            ..Default::default()
        }
    }

    pub fn modifier(theme: &Theme, status: Status, is_clicked: bool) -> Style {
        let palette = theme.extended_palette();

        Style {
            text_color: if status == Status::Disabled {
                palette.secondary.strong.color
            } else {
                palette.background.base.text
            },
            border: Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            background: match status {
                Status::Active => {
                    if is_clicked {
                        Some(palette.background.weak.color.into())
                    } else {
                        Some(palette.background.base.color.into())
                    }
                }
                Status::Pressed => Some(palette.background.strongest.color.into()),
                Status::Hovered => Some(palette.background.strong.color.into()),
                Status::Disabled => (!is_clicked).then(|| palette.background.weakest.color.into()),
            },
            ..Default::default()
        }
    }

    pub fn bordered_toggle(theme: &Theme, status: Status, is_active: bool) -> Style {
        let palette = theme.extended_palette();

        Style {
            text_color: if status == Status::Disabled {
                palette.background.stronger.color
            } else if is_active {
                palette.secondary.strong.color
            } else {
                palette.secondary.base.color
            },
            border: Border {
                radius: 3.0.into(),
                width: if is_active { 2.0 } else { 1.0 },
                color: palette.background.weak.color,
            },
            background: match status {
                Status::Active => {
                    if is_active {
                        Some(palette.background.base.color.into())
                    } else {
                        Some(palette.background.weakest.color.into())
                    }
                }
                Status::Pressed | Status::Disabled => Some(palette.background.weakest.color.into()),
                Status::Hovered => Some(palette.background.weak.color.into()),
            },
            ..Default::default()
        }
    }

    pub fn menu_body(theme: &Theme, status: Status, is_selected: bool) -> Style {
        let palette = theme.extended_palette();

        Style {
            text_color: palette.background.base.text,
            border: Border {
                radius: 3.0.into(),
                width: if is_selected { 2.0 } else { 0.0 },
                color: palette.background.strong.color,
            },
            background: match status {
                Status::Active => {
                    if is_selected {
                        Some(palette.background.base.color.into())
                    } else {
                        Some(palette.background.weakest.color.into())
                    }
                }
                Status::Pressed => Some(palette.background.base.color.into()),
                Status::Hovered => Some(palette.background.weak.color.into()),
                Status::Disabled => (!is_selected).then(|| palette.secondary.base.color.into()),
            },
            ..Default::default()
        }
    }
}

pub fn pane_grid(theme: &Theme) -> widget::pane_grid::Style {
    let palette = theme.extended_palette();

    widget::pane_grid::Style {
        hovered_region: Highlight {
            background: palette.background.strongest.color.scale_alpha(0.5).into(),
            border: Border {
                width: 1.0,
                color: palette.background.strongest.color,
                radius: 4.0.into(),
            },
        },
        picked_split: Line {
            color: palette.primary.strong.color,
            width: 4.0,
        },
        hovered_split: Line {
            color: palette.primary.weak.color,
            width: 4.0,
        },
    }
}

pub fn pane_title_bar(theme: &Theme) -> Style {
    let palette = theme.extended_palette();

    Style {
        background: if palette.is_dark {
            Some(palette.background.weak.color.scale_alpha(0.2).into())
        } else {
            Some(palette.background.strong.color.scale_alpha(0.2).into())
        },
        ..Default::default()
    }
}

pub fn pane_background(theme: &Theme, is_focused: bool) -> Style {
    let palette = theme.extended_palette();

    let color = if palette.is_dark {
        palette.background.weak.color
    } else {
        palette.background.strong.color
    };

    Style {
        text_color: Some(palette.background.base.text),
        background: Some(palette.background.weakest.color.into()),
        border: if is_focused {
            Border {
                width: 1.0,
                color: palette.background.strong.color,
                radius: 4.0.into(),
            }
        } else {
            Border {
                width: 1.0,
                color: color.scale_alpha(0.5),
                radius: 2.0.into(),
            }
        },
        ..Default::default()
    }
}

pub fn chart_modal(theme: &Theme) -> Style {
    let palette = theme.extended_palette();

    Style {
        text_color: Some(palette.background.base.text),
        background: Some(
            Color {
                a: 0.99,
                ..palette.background.base.color
            }
            .into(),
        ),
        border: Border {
            width: 1.0,
            color: palette.background.weak.color,
            radius: 4.0.into(),
        },
        shadow: Shadow {
            offset: iced::Vector { x: 0.0, y: 0.0 },
            blur_radius: 12.0,
            color: Color::BLACK.scale_alpha(if palette.is_dark { 0.4 } else { 0.2 }),
        },
        snap: true,
    }
}

pub fn dashboard_modal(theme: &Theme) -> Style {
    let palette = theme.extended_palette();

    Style {
        background: Some(
            Color {
                a: 0.99,
                ..palette.background.base.color
            }
            .into(),
        ),
        border: Border {
            width: 1.0,
            color: palette.background.weak.color,
            radius: 4.0.into(),
        },
        shadow: Shadow {
            offset: iced::Vector { x: 0.0, y: 0.0 },
            blur_radius: 20.0,
            color: Color::BLACK.scale_alpha(if palette.is_dark { 0.8 } else { 0.4 }),
        },
        ..Default::default()
    }
}

/// Green for buys, red for sells, the text color when the side is unknown.
pub fn side_text(theme: &Theme, side: Option<svp_common::protocol::Side>) -> widget::text::Style {
    let palette = theme.extended_palette();
    widget::text::Style {
        color: match side {
            Some(svp_common::protocol::Side::Buy) => Some(palette.success.base.color),
            Some(svp_common::protocol::Side::Sell) => Some(palette.danger.base.color),
            None => None,
        },
    }
}
