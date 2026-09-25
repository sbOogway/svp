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

    pub fn info(theme: &Theme, _status: Status) -> Style {
        let palette = theme.extended_palette();

        Style {
            text_color: palette.background.base.text,
            border: Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            background: Some(palette.background.weakest.color.into()),
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

pub fn modal_container(theme: &Theme) -> Style {
    let palette = theme.extended_palette();

    Style {
        text_color: Some(palette.background.base.text),
        background: Some(palette.background.weakest.color.into()),
        border: Border {
            width: 1.0,
            color: palette.background.weak.color,
            radius: 4.0.into(),
        },
        shadow: Shadow {
            offset: iced::Vector { x: 0.0, y: 0.0 },
            blur_radius: 2.0,
            color: Color::BLACK.scale_alpha(if palette.is_dark { 0.8 } else { 0.2 }),
        },
        snap: true,
    }
}

pub fn split_ruler(theme: &Theme) -> widget::rule::Style {
    let palette = theme.extended_palette();

    widget::rule::Style {
        color: palette.background.strong.color.scale_alpha(0.25),
        radius: iced::border::Radius::default(),
        fill_mode: widget::rule::FillMode::Full,
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

pub fn darken(color: Color, amount: f32) -> Color {
    let mut hsl = to_hsl(color);
    hsl.l = (hsl.l - amount).max(0.0);
    from_hsl(&hsl)
}

pub fn lighten(color: Color, amount: f32) -> Color {
    let mut hsl = to_hsl(color);
    hsl.l = (hsl.l + amount).min(1.0);
    from_hsl(&hsl)
}

struct Hsl {
    h: f32,
    s: f32,
    l: f32,
    a: f32,
}

#[allow(clippy::float_cmp)]
fn to_hsl(color: Color) -> Hsl {
    let x_max = color.r.max(color.g).max(color.b);
    let x_min = color.r.min(color.g).min(color.b);
    let c = x_max - x_min;
    let l = x_max.midpoint(x_min);

    let h = if c == 0.0 {
        0.0
    } else if x_max == color.r {
        60.0 * ((color.g - color.b) / c).rem_euclid(6.0)
    } else if x_max == color.g {
        60.0 * (((color.b - color.r) / c) + 2.0)
    } else {
        60.0 * (((color.r - color.g) / c) + 4.0)
    };

    let s = if l == 0.0 || l == 1.0 {
        0.0
    } else {
        (x_max - l) / l.min(1.0 - l)
    };

    Hsl {
        h,
        s,
        l,
        a: color.a,
    }
}

// https://en.wikipedia.org/wiki/HSL_and_HSV#HSL_to_RGB
fn from_hsl(hsl: &Hsl) -> Color {
    let c = (1.0 - (2.0 * hsl.l - 1.0).abs()) * hsl.s;
    let h = hsl.h / 60.0;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());

    let (r1, g1, b1) = if h < 1.0 {
        (c, x, 0.0)
    } else if h < 2.0 {
        (x, c, 0.0)
    } else if h < 3.0 {
        (0.0, c, x)
    } else if h < 4.0 {
        (0.0, x, c)
    } else if h < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    let m = hsl.l - (c / 2.0);

    Color {
        r: r1 + m,
        g: g1 + m,
        b: b1 + m,
        a: hsl.a,
    }
}
