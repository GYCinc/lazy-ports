// SPDX-License-Identifier: MIT

use crate::config::{Config, ServiceConfig, parse_address};
use crate::fl;
use crate::ports;
use crate::services::{self, Health};
#[path = "cbar.rs"]
mod cbar;
use cosmic::cosmic_config::{ConfigSet, CosmicConfigEntry};
use cosmic::iced::core::text::Span;
use cosmic::iced::futures;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{Length, Limits, Subscription, window::Id};
use cosmic::prelude::*;
use cosmic::widget::{self, icon, settings, text_input};
use futures::{SinkExt, StreamExt};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const APP_ID: &str = "io.github.hayron.lazy-ports";

const CORRUPT_CONFIG_MSG: &str = "Service config is corrupt — services are read-only until reset.";

fn action(m: Message) -> cosmic::Action<Message> {
    cosmic::Action::from(m)
}

// --- Tetradic Pastel Chassis Palette (#8ACFD6, #B78AD6, #D6918A, #A9D68A) ---
const LED_AMBER: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.961,
    g: 0.702,
    b: 0.263,
    a: 1.0,
}; // #f5b343 starting
const LED_RED: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.839,
    g: 0.569,
    b: 0.541,
    a: 1.0,
}; // #D6918A Soft Coral / Failed / Stop
const LED_CYAN: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.541,
    g: 0.812,
    b: 0.839,
    a: 1.0,
}; // #8ACFD6 Soft Cyan / Listening / Browser
const LED_PURPLE: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.718,
    g: 0.541,
    b: 0.839,
    a: 1.0,
}; // #B78AD6 Soft Lavender / Inactive
const LED_GREEN: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.663,
    g: 0.839,
    b: 0.541,
    a: 1.0,
}; // #A9D68A Soft Sage Green / Active / Start
const COLOR_CHASSIS: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.078,
    g: 0.086,
    b: 0.106,
    a: 1.0,
}; // #14161b main chassis fill
const COLOR_SURFACE: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.106,
    g: 0.122,
    b: 0.145,
    a: 1.0,
}; // #1b1f25 row-card plate: one clean step up from the chassis
const COLOR_VIEWPORT: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.047,
    g: 0.055,
    b: 0.071,
    a: 1.0,
}; // #0c0e12 drawer / pressed
const COLOR_PANEL: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.067,
    g: 0.075,
    b: 0.090,
    a: 1.0,
}; // #111317 header plate
const COLOR_FOOTER: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.063,
    g: 0.071,
    b: 0.090,
    a: 1.0,
}; // #101217 footer plate
const COLOR_BORDER: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.145,
    g: 0.161,
    b: 0.196,
    a: 1.0,
}; // #252932
const COLOR_BORDER_LIGHT: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.196,
    g: 0.216,
    b: 0.259,
    a: 1.0,
}; // #323742
const COLOR_DANGER: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.184,
    g: 0.078,
    b: 0.125,
    a: 1.0,
}; // ~#2f1420 rose-950/60 plate
const COLOR_SUBTLE: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.384,
    g: 0.408,
    b: 0.459,
    a: 1.0,
}; // #626875 separators/labels
const COLOR_DIM: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.545,
    g: 0.576,
    b: 0.627,
    a: 1.0,
}; // #8b93a0 secondary telemetry
const COLOR_BRIGHT: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.902,
    g: 0.929,
    b: 0.953,
    a: 1.0,
}; // #e6edf3 titles
const COLOR_KEY: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.831,
    g: 0.831,
    b: 0.847,
    a: 1.0,
}; // #d4d4d8 icons/badge text

const COLOR_TEXT2: cosmic::iced::Color = cosmic::iced::Color {
    r: 0.667,
    g: 0.706,
    b: 0.753,
    a: 1.0,
}; // #aab4c0 secondary telemetry — the one readable mid-tone

/// A user-selectable accent. The industrial dark chassis is fixed; only the
/// accent moves — the hero port, the running LED, its bloom and the start key.
#[derive(Clone, Copy, PartialEq)]
pub struct Accent {
    pub label: &'static str,
    pub color: cosmic::iced::Color,
}

const ACCENT_EMERALD: Accent = Accent {
    label: "Sage",
    color: LED_GREEN,
};
const ACCENT_AMBER: Accent = Accent {
    label: "Amber",
    color: LED_AMBER,
};
const ACCENT_CYAN: Accent = Accent {
    label: "Cyan",
    color: LED_CYAN,
};
const ACCENT_ROSE: Accent = Accent {
    label: "Coral",
    color: LED_RED,
};
const ACCENT_VIOLET: Accent = Accent {
    label: "Lavender",
    color: LED_PURPLE,
};

pub const ACCENTS: [Accent; 5] = [
    ACCENT_EMERALD,
    ACCENT_AMBER,
    ACCENT_CYAN,
    ACCENT_ROSE,
    ACCENT_VIOLET,
];

/// Resolve a persisted accent name, falling back to emerald for anything unknown.
fn accent_from_name(name: &str) -> Accent {
    ACCENTS
        .iter()
        .copied()
        .find(|a| a.label.eq_ignore_ascii_case(name.trim()))
        .unwrap_or(ACCENT_EMERALD)
}

fn status_led<'a, M: 'static>(color: cosmic::iced::Color) -> Element<'a, M> {
    widget::container(
        widget::Space::new()
            .width(Length::Fixed(8.0))
            .height(Length::Fixed(8.0)),
    )
    .width(Length::Fixed(8.0))
    .height(Length::Fixed(8.0))
    .class(cosmic::theme::Container::Custom(Box::new(move |_| {
        cosmic::iced::widget::container::Style {
            background: Some(cosmic::iced::Background::Color(color)),
            border: cosmic::iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })))
    .into()
}

/// Bold monospace font for the hero port value.
fn mono_bold() -> cosmic::iced::Font {
    cosmic::iced::Font {
        weight: cosmic::iced::font::Weight::Bold,
        ..cosmic::font::mono()
    }
}

/// Clickable row title: 13.0 px semibold text that opens the URL on press.
/// Highlights in soft cyan on hover.
fn link_title<'a>(title: String, url: String) -> Element<'a, Message> {
    let txt = widget::Text::new(title)
        .size(13.0)
        .font(cosmic::font::semibold())
        .wrapping(cosmic::iced::core::text::Wrapping::None);

    let btn = widget::button::custom(txt)
        .padding(0)
        .class(cosmic::theme::Button::Custom {
            active: Box::new(|_, _| key_style_flat(COLOR_BRIGHT)),
            disabled: Box::new(|_| key_style_flat(COLOR_DIM)),
            hovered: Box::new(|_, _| key_style_flat(LED_CYAN)),
            pressed: Box::new(|_, _| key_style_flat(COLOR_BRIGHT)),
        })
        .on_press(Message::Open(url.clone()));

    tip(btn, format!("Open {url}"))
}

/// Plain row title: 13.0 px semibold text matching link_title typography.
fn plain_title<'a>(title: String, industrial: bool) -> Element<'a, Message> {
    let txt = widget::Text::new(title)
        .size(13.0)
        .font(cosmic::font::semibold())
        .wrapping(cosmic::iced::core::text::Wrapping::None);
    if industrial {
        txt.class(cosmic::theme::Text::Color(COLOR_BRIGHT)).into()
    } else {
        txt.into()
    }
}

/// Secondary telemetry: 12 px monospace, readable mid-tone.
fn dim_mono<'a, M: 'static>(s: impl Into<String> + 'a) -> Element<'a, M> {
    widget::Text::new(s.into())
        .size(12.0)
        .font(cosmic::font::mono())
        .wrapping(cosmic::iced::core::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(COLOR_TEXT2))
        .into()
}

/// Faint label or separator: 11 px monospace, chassis subtle.
fn faint_mono<'a, M: 'static>(s: impl Into<String> + 'a) -> Element<'a, M> {
    widget::Text::new(s.into())
        .size(11.0)
        .font(cosmic::font::mono())
        .wrapping(cosmic::iced::core::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(COLOR_SUBTLE))
        .into()
}

/// Endpoint line: `localhost:3000` on the left, `PID 93579` / telemetry on the far right.
fn endpoint_line<'a, M: 'static>(
    host: &'a str,
    port: String,
    spec: String,
    port_color: cosmic::iced::Color,
) -> Element<'a, M> {
    let clean_host = host.trim_end_matches(':').trim();
    let host_prefix = if clean_host.is_empty() {
        String::new()
    } else {
        format!("{clean_host}:")
    };
    let dim: Span<'_, ()> = Span::new(host_prefix)
        .size(12.0)
        .font(cosmic::font::mono())
        .color(COLOR_TEXT2);
    let hero: Span<'_, ()> = Span::new(port)
        .size(14.0)
        .font(mono_bold())
        .color(port_color);

    let left = cosmic::iced::widget::text::Rich::with_spans([dim, hero])
        .wrapping(cosmic::iced::core::text::Wrapping::None);

    if spec.is_empty() {
        left.into()
    } else {
        let right = widget::Text::new(spec)
            .size(12.0)
            .font(cosmic::font::mono())
            .wrapping(cosmic::iced::core::text::Wrapping::None)
            .class(cosmic::theme::Text::Color(COLOR_TEXT2));

        widget::row::with_capacity(3)
            .width(Length::Fill)
            .align_y(cosmic::iced::Alignment::Center)
            .push(left)
            .push(widget::Space::new().width(Length::Fill))
            .push(right)
            .into()
    }
}


/// Same chip, but clicking it filters the whole list to that process.
fn filter_badge<'a>(process: String, text: String) -> Element<'a, Message> {
    let chip = badge_plate_widget(text);
    tip(
        widget::button::custom(chip)
            .padding(0)
            .class(badge_button_class())
            .on_press(Message::FilterTo(process.clone())),
        format!("Filter by {process}"),
    )
}

/// Crisp uppercase tag: subtle surface background, 1px border, high-contrast readable text.
fn badge_plate_widget<'a, M: 'static>(s: String) -> Element<'a, M> {
    let txt = widget::Text::new(s.to_uppercase())
        .size(11.0)
        .font(cosmic::font::semibold())
        .wrapping(cosmic::iced::core::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(COLOR_KEY));

    widget::container(txt)
        .padding([2, 6])
        .class(cosmic::theme::Container::Custom(Box::new(|_| {
            cosmic::iced::widget::container::Style {
                background: Some(cosmic::iced::Background::Color(COLOR_VIEWPORT)),
                border: cosmic::iced::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: COLOR_BORDER,
                },
                ..Default::default()
            }
        })))
        .into()
}

/// Badge button: highlights on hover.
fn badge_button_class() -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(|_, _| cosmic::widget::button::Style::new()),
        disabled: Box::new(|_| cosmic::widget::button::Style::new()),
        hovered: Box::new(|_, _| {
            let mut style = cosmic::widget::button::Style::new();
            style.icon_color = Some(COLOR_BRIGHT);
            style.text_color = Some(COLOR_BRIGHT);
            style
        }),
        pressed: Box::new(|_, _| cosmic::widget::button::Style::new()),
    }
}

/// Pin button anchored in the upper-right corner of a card.
fn pin_circle_button<'a>(msg: Message, pinned: bool, accent: Accent) -> Element<'a, Message> {
    let glyph = widget::container(icon::from_name("view-pin-symbolic").size(14))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(cosmic::iced::Alignment::Center)
        .align_y(cosmic::iced::Alignment::Center);
    tip(
        widget::button::custom(glyph)
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(24.0))
            .padding(0)
            .class(pin_button_class(pinned, accent))
            .on_press(msg),
        if pinned {
            "Pinned service · click to edit"
        } else {
            "Pin as Lazy Ports service"
        },
    )
}

fn pin_button_class(pinned: bool, accent: Accent) -> cosmic::theme::Button {
    let base_ink = if pinned { accent.color } else { COLOR_TEXT2 };
    cosmic::theme::Button::Custom {
        active: Box::new(move |_, _| cosmic::widget::button::Style {
            background: None,
            border_width: 0.0,
            border_color: cosmic::iced::Color::TRANSPARENT,
            border_radius: 4.0.into(),
            icon_color: Some(base_ink),
            text_color: Some(base_ink),
            ..Default::default()
        }),
        disabled: Box::new(|_| key_style_flat(COLOR_DIM)),
        hovered: Box::new(move |_, _| cosmic::widget::button::Style {
            background: Some(cosmic::iced::Background::Color(COLOR_BORDER)),
            border_width: 1.0,
            border_color: COLOR_BORDER_LIGHT,
            border_radius: 4.0.into(),
            icon_color: Some(if pinned { accent.color } else { COLOR_BRIGHT }),
            text_color: Some(if pinned { accent.color } else { COLOR_BRIGHT }),
            ..Default::default()
        }),
        pressed: Box::new(move |_, _| cosmic::widget::button::Style {
            background: Some(cosmic::iced::Background::Color(COLOR_VIEWPORT)),
            border_width: 1.0,
            border_color: COLOR_BORDER,
            border_radius: 4.0.into(),
            icon_color: Some(base_ink),
            text_color: Some(base_ink),
            ..Default::default()
        }),
    }
}

/// Inset telemetry drawer: viewport plate, 1 px border, 4 px radius.
fn drawer_plate() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(|_| cosmic::iced::widget::container::Style {
        background: Some(cosmic::iced::Background::Color(COLOR_VIEWPORT)),
        border: cosmic::iced::Border {
            radius: 4.0.into(),
            width: 1.0,
            color: COLOR_BORDER,
        },
        ..Default::default()
    })
}

/// One milled-key style state.
fn key_style(
    plate: cosmic::iced::Color,
    border: cosmic::iced::Color,
    ink: cosmic::iced::Color,
) -> cosmic::widget::button::Style {
    let mut style = cosmic::widget::button::Style::new();
    style.background = Some(cosmic::iced::Background::Color(plate));
    style.border_width = 1.0;
    style.border_color = border;
    style.border_radius = 4.0.into();
    style.icon_color = Some(ink);
    style.text_color = Some(ink);
    style
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyColor {
    Neutral,
    Blue,
    Green,
    Danger,
}

/// Milled key button class: flat glyph at rest, plate on hover.
/// Neutral / light blue (browser) / emerald (start) / rose (kill).
fn key_button_class(color: KeyColor, accent: Accent) -> cosmic::theme::Button {
    let base_ink = match color {
        KeyColor::Green => accent.color,
        KeyColor::Danger => LED_RED,
        KeyColor::Blue => LED_CYAN,
        KeyColor::Neutral => COLOR_TEXT2,
    };
    let (hover_plate, hover_border, hover_ink) = match color {
        KeyColor::Danger => (COLOR_DANGER, LED_RED, LED_RED),
        KeyColor::Blue => (COLOR_BORDER, LED_CYAN, LED_CYAN),
        KeyColor::Green => (COLOR_BORDER, accent.color, accent.color),
        KeyColor::Neutral => (COLOR_BORDER, COLOR_BORDER_LIGHT, COLOR_BRIGHT),
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |_, _| key_style_flat(base_ink)),
        disabled: Box::new(|_| key_style_flat(COLOR_DIM)),
        hovered: Box::new(move |_, _| key_style(hover_plate, hover_border, hover_ink)),
        pressed: Box::new(move |_, _| key_style(COLOR_VIEWPORT, COLOR_BORDER_LIGHT, hover_ink)),
    }
}

/// Flat button state: ink only, no plate, no border.
fn key_style_flat(ink: cosmic::iced::Color) -> cosmic::widget::button::Style {
    let mut style = cosmic::widget::button::Style::new();
    style.icon_color = Some(ink);
    style.text_color = Some(ink);
    style
}

/// Hover tooltip chip. Glyph-only keys and any value that can outgrow the
/// 476 px chassis are wrapped in one of these, so a truncated service name or
/// a 14 px glyph is still legible on hover.
fn tip<'a, M: 'static>(
    content: impl Into<Element<'a, M>>,
    label: impl Into<String> + 'a,
) -> Element<'a, M> {
    widget::tooltip(
        content,
        widget::Text::new(label.into())
            .size(11.0)
            .font(cosmic::font::mono())
            .class(cosmic::theme::Text::Color(COLOR_BRIGHT)),
        widget::tooltip::Position::Bottom,
    )
    .gap(6)
    .padding(6)
    .into()
}

/// 28 px square milled key with a 14 px glyph, optically centered.
/// `label` is the hover tooltip — glyph-only controls must never ship without one.
fn key_button<'a>(
    name: &str,
    label: &'a str,
    accent: Accent,
    msg: Message,
    color: KeyColor,
) -> Element<'a, Message> {
    let base_ink = match color {
        KeyColor::Green => accent.color,
        KeyColor::Danger => LED_RED,
        KeyColor::Blue => LED_CYAN,
        KeyColor::Neutral => COLOR_TEXT2,
    };
    let glyph = widget::container(icon::from_name(name).size(14))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(cosmic::iced::Alignment::Center)
        .align_y(cosmic::iced::Alignment::Center)
        .class(cosmic::theme::Container::Custom(Box::new(move |_| {
            cosmic::iced::widget::container::Style {
                icon_color: Some(base_ink),
                ..Default::default()
            }
        })));
    tip(
        widget::button::custom(glyph)
            .width(Length::Fixed(28.0))
            .height(Length::Fixed(28.0))
            .padding(0)
            .class(key_button_class(color, accent))
            .on_press(msg),
        label.to_string(),
    )
}

/// Flat icon key: no plate, no border in any state — a glyph affordance, not
/// button chrome.
fn flat_key_button<'a>(name: &str, label: &'a str, msg: Message) -> Element<'a, Message> {
    let glyph = widget::container(icon::from_name(name).size(14))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(cosmic::iced::Alignment::Center)
        .align_y(cosmic::iced::Alignment::Center);
    tip(
        widget::button::custom(glyph)
            .width(Length::Fixed(28.0))
            .height(Length::Fixed(28.0))
            .padding(0)
            .class(cosmic::theme::Button::Custom {
                active: Box::new(|_, _| flat_btn_style(COLOR_TEXT2)),
                disabled: Box::new(|_| flat_btn_style(COLOR_SUBTLE)),
                hovered: Box::new(|_, _| flat_btn_style(COLOR_BRIGHT)),
                pressed: Box::new(|_, _| flat_btn_style(COLOR_TEXT2)),
            })
            .on_press(msg),
        label.to_string(),
    )
}

/// Bare button style: transparent plate and border, ink only.
fn flat_btn_style(ink: cosmic::iced::Color) -> cosmic::widget::button::Style {
    let mut style = cosmic::widget::button::Style::new();
    style.icon_color = Some(ink);
    style
}

/// A standalone named icon tinted to `color` (icons take their color from the
/// enclosing container style, not from the icon itself).
fn tinted_icon<'a>(name: &str, size: u16, color: cosmic::iced::Color) -> Element<'a, Message> {
    widget::container(icon::from_name(name).size(size))
        .class(cosmic::theme::Container::Custom(Box::new(move |_| {
            cosmic::iced::widget::container::Style {
                icon_color: Some(color),
                ..Default::default()
            }
        })))
        .into()
}

/// Compact mono utility button for the footer.
fn key_text_button<'a>(label: impl Into<String> + 'a, msg: Message) -> Element<'a, Message> {
    key_glyph_text_button(None, label, msg)
}

/// Footer utility button with a leading glyph, e.g. Rescan.
fn key_glyph_text_button<'a>(
    glyph: Option<&'a str>,
    label: impl Into<String> + 'a,
    msg: Message,
) -> Element<'a, Message> {
    let text = widget::Text::new(label.into())
        .size(13.0)
        .font(mono_bold())
        .wrapping(cosmic::iced::core::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(COLOR_KEY));

    let content: Element<'a, Message> = match glyph {
        Some(name) => widget::row::with_capacity(2)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(6.0)
            .push(tinted_icon(name, 14, COLOR_KEY))
            .push(text)
            .into(),
        None => text.into(),
    };

    widget::button::custom(content)
        .padding([4, 10])
        .class(key_button_class(KeyColor::Neutral, ACCENT_EMERALD))
        .on_press(msg)
        .into()
}

/// Phosphor LED: 8 px circular bullet with a soft bloom.
fn led_glow<'a, M: 'static>(color: cosmic::iced::Color) -> Element<'a, M> {
    widget::container(
        widget::Space::new()
            .width(Length::Fixed(8.0))
            .height(Length::Fixed(8.0)),
    )
    .width(Length::Fixed(8.0))
    .height(Length::Fixed(8.0))
    .class(cosmic::theme::Container::Custom(Box::new(move |_| {
        cosmic::iced::widget::container::Style {
            background: Some(cosmic::iced::Background::Color(color)),
            border: cosmic::iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            shadow: cosmic::iced::Shadow {
                color,
                offset: cosmic::iced::Vector::ZERO,
                blur_radius: 4.0,
            },
            ..Default::default()
        }
    })))
    .into()
}

/// Settings key for a refresh mode; the active mode lights up in the accent.
fn refresh_rate_key<'a>(
    secs: u64,
    label: String,
    current: u64,
    accent: Accent,
) -> Element<'a, Message> {
    let color = if current == secs {
        KeyColor::Green
    } else {
        KeyColor::Neutral
    };
    tip(
        widget::button::custom(
            widget::Text::new(label)
                .size(12.0)
                .font(cosmic::font::mono()),
        )
        .padding([4, 10])
        .class(key_button_class(color, accent))
        .on_press(Message::SetRefresh(secs)),
        format!("{secs}s interval"),
    )
}

/// Ringed magnifier: the search hold-in spot. A 1 px hairline ring on a
/// transparent fill — the only search chrome there is.
fn search_ring() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(|_| cosmic::iced::widget::container::Style {
        border: cosmic::iced::Border {
            radius: 13.0.into(),
            width: 1.0,
            color: COLOR_BORDER_LIGHT,
        },
        ..Default::default()
    })
}

/// Borderless filter field: transparent in every state, no placeholder text.
/// It sits beside the magnifier ring, which carries the affordance.
fn filter_input_class() -> cosmic::theme::TextInput {
    fn clear() -> cosmic::widget::text_input::Appearance {
        cosmic::widget::text_input::Appearance {
            background: cosmic::iced::Background::Color(cosmic::iced::Color::TRANSPARENT),
            border_radius: 0.0.into(),
            border_offset: None,
            border_width: 0.0,
            border_color: cosmic::iced::Color::TRANSPARENT,
            label_color: COLOR_TEXT2,
            placeholder_color: cosmic::iced::Color::TRANSPARENT,
            selected_text_color: COLOR_BRIGHT,
            icon_color: Some(COLOR_TEXT2),
            text_color: Some(COLOR_BRIGHT),
            selected_fill: COLOR_BORDER,
        }
    }
    cosmic::theme::TextInput::Custom {
        active: Box::new(|_| clear()),
        error: Box::new(|_| clear()),
        hovered: Box::new(|_| clear()),
        focused: Box::new(|_| clear()),
        disabled: Box::new(|_| clear()),
    }
}

/// Row card: one elevation step over the chassis with a single hairline.
/// Hover brightens the hairline only — no fill jump, no second gray.
fn row_card(hovered: bool) -> cosmic::theme::Container<'static> {
    let border = if hovered {
        COLOR_BORDER_LIGHT
    } else {
        COLOR_BORDER
    };
    cosmic::theme::Container::custom(move |_| cosmic::iced::widget::container::Style {
        background: Some(cosmic::iced::Background::Color(COLOR_SURFACE)),
        border: cosmic::iced::Border {
            radius: 6.0.into(),
            width: 1.0,
            color: border,
        },
        ..Default::default()
    })
}

/// Theme-aware row plate: legacy dark hairline card in industrial mode,
/// theme `Card` in system mode (follows COSMIC light/dark + accent).
fn card_class(hovered: bool, industrial: bool) -> cosmic::theme::Container<'static> {
    if industrial {
        row_card(hovered)
    } else {
        cosmic::theme::Container::Card
    }
}

/// Theme-aware drawer plate: viewport inset in industrial, `Secondary` on system.
fn drawer_class(industrial: bool) -> cosmic::theme::Container<'static> {
    if industrial {
        drawer_plate()
    } else {
        cosmic::theme::Container::Secondary
    }
}

/// Theme-aware badge: dark viewport chip in industrial, `Secondary` chip on system.
fn themed_badge<'a, M: 'static>(s: String, industrial: bool) -> Element<'a, M> {
    if industrial {
        return badge_plate_widget(s);
    }
    let txt = widget::Text::new(s.to_uppercase())
        .size(11.0)
        .font(cosmic::font::semibold())
        .wrapping(cosmic::iced::core::text::Wrapping::None);
    widget::container(txt)
        .padding([2, 6])
        .class(cosmic::theme::Container::Secondary)
        .into()
}

/// Theme-aware endpoint line: legacy hero-port Rich line in industrial;
/// plain theme text with fill-separated spec on system (no hardcoded colors,
/// so light themes stay legible and nothing can overlay the actions row).
fn themed_endpoint<'a, M: 'static>(
    port_label: String,
    spec: String,
    port_color: cosmic::iced::Color,
    industrial: bool,
) -> Element<'a, M> {
    if industrial {
        return endpoint_line("localhost", port_label, spec, port_color);
    }
    if spec.is_empty() {
        return widget::Text::new(port_label)
            .size(13.0)
            .font(cosmic::font::mono())
            .wrapping(cosmic::iced::core::text::Wrapping::None)
            .into();
    }
    widget::row::with_capacity(3)
        .width(Length::Fill)
        .align_y(cosmic::iced::Alignment::Center)
        .push(
            widget::Text::new(port_label)
                .size(13.0)
                .font(mono_bold())
                .wrapping(cosmic::iced::core::text::Wrapping::None),
        )
        .push(widget::Space::new().width(Length::Fill))
        .push(
            widget::Text::new(spec)
                .size(12.0)
                .font(cosmic::font::mono())
                .wrapping(cosmic::iced::core::text::Wrapping::None),
        )
        .into()
}

/// One card shape for every row type: a title row (LED + clipped name + tag +
/// pin button + action keys) over an endpoint line that owns the full second row.
#[allow(clippy::too_many_arguments)]
fn card_shell<'a>(
    led: cosmic::iced::Color,
    title: Element<'a, Message>,
    badge: Element<'a, Message>,
    pin_btn: Option<Element<'a, Message>>,
    endpoint: Element<'a, Message>,
    actions: Element<'a, Message>,
    drawer: Option<Element<'a, Message>>,
    hovered: bool,
    hover_id: String,
    industrial: bool,
) -> Element<'a, Message> {
    let spacing = cosmic::theme::spacing();
    let title_left = widget::row::with_capacity(3)
        .align_y(cosmic::iced::Alignment::Center)
        .spacing(spacing.space_xs)
        .push(led_glow(led))
        .push(title)
        .push(badge);

    let mut actions_right = widget::row::with_capacity(3)
        .align_y(cosmic::iced::Alignment::Center)
        .spacing(6.0)
        .push(actions);
    if let Some(pin) = pin_btn {
        actions_right = actions_right.push(pin);
    }

    let title_row = widget::row::with_capacity(2)
        .align_y(cosmic::iced::Alignment::Center)
        .width(Length::Fill)
        .push(
            widget::container(title_left)
                .width(Length::Fill)
                .clip(true),
        )
        .push(actions_right);

    let mut card_col = widget::column::with_capacity(3)
        .spacing(4.0)
        .push(title_row)
        .push(endpoint);
    if let Some(d) = drawer {
        card_col = card_col.push(d);
    }

    let plate = widget::container(card_col)
        .padding([8, 10])
        .width(Length::Fill)
        .class(card_class(hovered, industrial));

    widget::mouse_area(plate)
        .on_enter(Message::HoverRow(Some(hover_id)))
        .on_exit(Message::HoverRow(None))
        .into()
}

/// Outer chassis: the `#14161b` instrument body with its hairline edge and
/// soft drop shadow. Without this every plate below paints onto transparency,
/// which is what made the popup read as a floating tab.
fn chassis_plate() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(|_| cosmic::iced::widget::container::Style {
        background: Some(cosmic::iced::Background::Color(COLOR_CHASSIS)),
        border: cosmic::iced::Border {
            radius: 12.0.into(),
            width: 1.0,
            color: COLOR_BORDER,
        },
        text_color: Some(COLOR_BRIGHT),
        ..Default::default()
    })
}

/// Flat structural plate for header / footer bands.
fn panel_plate(fill: cosmic::iced::Color, radius: f32) -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(move |_| cosmic::iced::widget::container::Style {
        background: Some(cosmic::iced::Background::Color(fill)),
        border: cosmic::iced::Border {
            radius: radius.into(),
            ..Default::default()
        },
        ..Default::default()
    })
}

/// A 1 px hairline rule, so adjacent bands read as separated instrument bays
/// instead of one smeared block.
fn seam() -> Element<'static, Message> {
    widget::container(widget::Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .class(panel_plate(COLOR_BORDER, 0.0))
        .into()
}

/// Square logo / badge tile with a hairline border.
/// Port of LocalPorts: a panel applet managing local dev services & ports.
pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    /// Popup id scheduled for a deferred destroy, still alive in the compositor.
    popup_closing: Option<Id>,
    config: Config,
    config_ctx: Option<cosmic::cosmic_config::Config>,
    services: Vec<ServiceConfig>,
    services_corrupt: bool,
    ports: Vec<ports::ListeningPort>,
    states: HashMap<String, SvcState>,
    health: HashMap<String, Health>,
    stopping: HashSet<String>,
    logs_open: Option<String>,
    log_text: String,
    show_add: bool,
    show_settings: bool,
    expanded_service: Option<String>,
    show_inactive: bool,
    /// Combine ports rows by owning process.
    group_ports: bool,
    edit_id: Option<String>,
    in_name: String,
    in_address: String,
    in_cmd: String,
    in_dir: String,
    in_reserved: bool,
    error: Option<String>,
    /// Header filter: matches service names and ports ("filter name or port...").
    filter: String,
    /// Row currently under the cursor (spec tactile lift on hover).
    hover: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum SvcState {
    Stopped,
    Starting,
    Running,
    Failed(String),
    Collision(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    DestroyPopup(Id),
    Tick,
    PortsRefreshed(Vec<ports::ListeningPort>),
    HealthBatch(Vec<(String, Health)>),
    StartService(String),
    StartResult(String, Result<u32, String>),
    StopService(String),
    StopResult(String),
    Exited(String, Option<i32>),
    Open(String),
    KillPort(i32),
    KillProcessGroup(Vec<i32>),
    PinPort(ports::ListeningPort),
    #[allow(dead_code)]
    IgnorePort(ports::ListeningPort),
    ClearIgnored,
    ToggleGroup,
    ToggleColloquial,
    ToggleHideSystem,
    ToggleShowPorts,
    ToggleShowInactive,
    ToggleSettings,
    ToggleExpandService(String),
    ToggleReservePort(String),
    RestartService(String),
    RestartResult(String, Result<(u32, bool), String>),
    ShowAdd,
    HideAdd,
    NameInput(String),
    AddressInput(String),
    CmdInput(String),
    DirInput(String),
    ReservedInput(bool),
    SaveService,
    DeleteService(String),
    MoveServiceUp(String),
    MoveServiceDown(String),
    ResetCorruptConfig,
    EditService(String),
    ShowLogs(String),
    HideLogs,
    LogText {
        id: String,
        text: String,
    },
    UpdateConfig(Config),
    QuitApplet,
    FilterInput(String),
    /// Jump the filter to a process name (badge chip click).
    FilterTo(String),
    /// Choose an explicit poll rate (settings view).
    SetRefresh(u64),
    /// Pick a theme mode: "system" (COSMIC default) or "industrial".
    SetThemeMode(String),
    HoverRow(Option<String>),
    Event(services::Event),
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let (config, config_ctx) = match cosmic::cosmic_config::Config::new(APP_ID, Config::VERSION)
        {
            Ok(ctx) => match Config::get_entry(&ctx) {
                Ok(config) => (config, Some(ctx)),
                Err((_errors, config)) => (config, Some(ctx)),
            },
            Err(_) => (Config::default(), None),
        };
        let (services, services_corrupt) = match parse_services(&config.services_json) {
            Ok(services) => (services, false),
            Err(_) => (Vec::new(), true),
        };

        let mut app = AppModel {
            core,
            popup: None,
            popup_closing: None,
            config,
            config_ctx,
            services,
            services_corrupt,
            ports: Vec::new(),
            states: HashMap::new(),
            health: HashMap::new(),
            stopping: HashSet::new(),
            logs_open: None,
            log_text: String::new(),
            show_add: false,
            show_settings: false,
            expanded_service: None,
            show_inactive: false,
            group_ports: true,
            edit_id: None,
            in_name: String::new(),
            in_address: String::new(),
            in_cmd: String::new(),
            in_dir: String::new(),
            in_reserved: false,
            error: services_corrupt.then(|| CORRUPT_CONFIG_MSG.to_string()),
            filter: String::new(),
            hover: None,
        };

        // First-run import from CBar's localports config.
        if !app.services_corrupt
            && app.services.is_empty()
            && let Some(imported) = cbar::load_services()
        {
            app.services = imported;
            if let Err(e) = app.persist_services() {
                app.error = Some(e);
            }
        }

        // Synchronize initial port reservations for any stopped services
        services::sync_reservations(&app.services);

        (
            app,
            Task::perform(ports::scan(), |v| action(Message::PortsRefreshed(v))),
        )
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Self::Message> {
        self.core
            .applet
            .icon_button("io.github.hayron.lazy-ports-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let spacing = cosmic::theme::spacing();

        // 1. Log viewer view
        if let Some(log_id) = self.logs_open.as_ref() {
            let name = self
                .services
                .iter()
                .find(|s| &s.id == log_id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| log_id.clone());

            let header = widget::row::with_capacity(3)
                .align_y(cosmic::iced::Alignment::Center)
                .spacing(spacing.space_xs)
                .push(flat_key_button(
                    "go-previous-symbolic",
                    "Back to Lazy Ports",
                    Message::HideLogs,
                ))
                .push(
                    widget::Text::new(format!("{name} Logs"))
                        .size(16.0)
                        .font(cosmic::font::semibold())
                        .class(cosmic::theme::Text::Color(COLOR_BRIGHT))
                        .width(Length::Fill),
                );

            let log_text = if self.log_text.is_empty() {
                fl!("log-empty")
            } else {
                self.log_text.clone()
            };

            let log_view = widget::container(
                widget::scrollable(
                    widget::text::body(log_text)
                        .font(cosmic::font::mono())
                        .size(11.5)
                        .class(cosmic::theme::Text::Color(COLOR_TEXT2))
                        .width(Length::Fill),
                )
                .class(cosmic::theme::style::iced::Scrollable::Minimal)
                .height(Length::Fill),
            )
            .padding(10)
            .width(Length::Fill)
            .height(Length::Fill)
            .class(panel_plate(COLOR_VIEWPORT, 6.0));

            let content = widget::column::with_capacity(4)
                .spacing(spacing.space_xs)
                .width(Length::Fill)
                .height(Length::Fill)
                .push(
                    widget::container(header)
                        .padding([10, 14])
                        .width(Length::Fill)
                        .class(panel_plate(COLOR_PANEL, 0.0)),
                )
                .push(seam())
                .push(
                    widget::container(log_view)
                        .padding([8, 12])
                        .width(Length::Fill)
                        .height(Length::Fill),
                );

            let chassis = widget::container(content)
                .clip(true)
                .class(chassis_plate())
                .width(Length::Fill)
                .height(Length::Fill);

            return self.core.applet.popup_container(chassis).into();
        }

        // 2. Settings / Preferences view
        if self.show_settings {
            let header = widget::row::with_capacity(3)
                .align_y(cosmic::iced::Alignment::Center)
                .spacing(spacing.space_xs)
                .push(flat_key_button(
                    "go-previous-symbolic",
                    "Back to Lazy Ports",
                    Message::ToggleSettings,
                ))
                .push(
                    widget::Text::new(fl!("preferences"))
                        .size(16.0)
                        .font(cosmic::font::semibold())
                        .class(if self.industrial() {
                            cosmic::theme::Text::Color(COLOR_BRIGHT)
                        } else {
                            cosmic::theme::Text::Default
                        })
                        .width(Length::Fill),
                );

            let mut pref_section = settings::section();
            pref_section = pref_section.add(
                settings::item::builder("Refresh Rate")
                    .description("Background polling frequency for listening ports")
                    .control(
                        widget::row::with_capacity(3)
                            .spacing(6)
                            .push(refresh_rate_key(
                                1,
                                fl!("refresh-mode-realtime"),
                                self.config.refresh_interval(),
                                self.accent(),
                            ))
                            .push(refresh_rate_key(
                                2,
                                fl!("refresh-mode-balanced"),
                                self.config.refresh_interval(),
                                self.accent(),
                            ))
                            .push(refresh_rate_key(
                                10,
                                fl!("refresh-mode-low"),
                                self.config.refresh_interval(),
                                self.accent(),
                            )),
                    ),
            );
            pref_section = pref_section.add(
                settings::item::builder(fl!("group-ports"))
                    .description("Combine multi-port sockets from the same process")
                    .control(
                        widget::toggler(self.group_ports).on_toggle(|_| Message::ToggleGroup),
                    ),
            );
            pref_section = pref_section.add(
                settings::item::builder(fl!("hide-system"))
                    .description("Filter out system daemons (systemd, cups, avahi, etc.)")
                    .control(
                        widget::toggler(self.config.hide_system).on_toggle(|_| Message::ToggleHideSystem),
                    ),
            );

            let industrial = self.industrial();
            let theme_control = widget::row::with_capacity(2)
                .spacing(6)
                .push(
                    widget::button::standard("System")
                        .class(if industrial {
                            cosmic::theme::Button::Standard
                        } else {
                            cosmic::theme::Button::Suggested
                        })
                        .on_press(Message::SetThemeMode("system".to_string())),
                )
                .push(
                    widget::button::standard("Industrial")
                        .class(if industrial {
                            cosmic::theme::Button::Suggested
                        } else {
                            cosmic::theme::Button::Standard
                        })
                        .on_press(Message::SetThemeMode("industrial".to_string())),
                );
            pref_section = pref_section.add(
                settings::item::builder("Theme")
                    .description("System follows COSMIC; Industrial keeps the legacy dark plates")
                    .control(theme_control),
            );
            pref_section = pref_section.add(
                settings::item::builder("Colloquial Slang")
                    .description("Show playful status labels (PIMPIN', SLEEPIN', COOKED)")
                    .control(
                        widget::toggler(self.config.colloquial).on_toggle(|_| Message::ToggleColloquial),
                    ),
            );

            let ignored = self.config.ignored_processes();
            if !ignored.is_empty() {
                pref_section = pref_section.add(
                    settings::item::builder(format!("Ignored Processes ({})", ignored.len()))
                        .description(ignored.join(", "))
                        .control(
                            widget::button::destructive(fl!("clear-ignored")).on_press(Message::ClearIgnored),
                        ),
                );
            }

            let about_footer = widget::row::with_capacity(1)
                .width(Length::Fill)
                .align_y(cosmic::iced::Alignment::Center)
                .push(
                    widget::Text::new("Lazy Ports v0.1.0 · Industrial Precision Port Manager")
                        .size(11.0)
                        .font(cosmic::font::mono())
                        .class(if industrial {
                            cosmic::theme::Text::Color(COLOR_SUBTLE)
                        } else {
                            cosmic::theme::Text::Default
                        }),
                );

            if industrial {
                let content = widget::column::with_capacity(6)
                    .spacing(spacing.space_xs)
                    .width(Length::Fill)
                    .push(
                        widget::container(header)
                            .padding([10, 14])
                            .width(Length::Fill)
                            .class(panel_plate(COLOR_PANEL, 0.0)),
                    )
                    .push(seam())
                    .push(
                        widget::container(pref_section)
                            .padding([8, 12])
                            .width(Length::Fill),
                    )
                    .push(seam())
                    .push(
                        widget::container(about_footer)
                            .padding([10, 14])
                            .width(Length::Fill)
                            .class(panel_plate(COLOR_FOOTER, 0.0)),
                    );

                let chassis = widget::container(
                    widget::scrollable(content)
                        .class(cosmic::theme::style::iced::Scrollable::Minimal),
                )
                .clip(true)
                .class(chassis_plate())
                .width(Length::Fill)
                .height(Length::Fill);

                return self.core.applet.popup_container(chassis).into();
            }

            let content = widget::column::with_capacity(4)
                .spacing(spacing.space_xs)
                .width(Length::Fill)
                .push(
                    widget::container(header)
                        .padding([10, 14])
                        .width(Length::Fill),
                )
                .push(
                    widget::container(pref_section)
                        .padding([8, 12])
                        .width(Length::Fill),
                )
                .push(
                    widget::container(about_footer)
                        .padding([10, 14])
                        .width(Length::Fill),
                );

            let scroll = widget::scrollable(
                widget::container(content)
                    .padding([10, 12])
                    .width(Length::Fill),
            );
            return self.core.applet.popup_container(scroll).into();
        }

        // 3. Add / Edit form view
        if self.show_add {
            let title = if self.edit_id.is_some() {
                fl!("edit-service")
            } else {
                fl!("add-service")
            };

            let header = widget::row::with_capacity(3)
                .align_y(cosmic::iced::Alignment::Center)
                .spacing(spacing.space_xs)
                .push(flat_key_button(
                    "go-previous-symbolic",
                    "Back to Lazy Ports",
                    Message::HideAdd,
                ))
                .push(
                    widget::Text::new(title)
                        .size(16.0)
                        .font(cosmic::font::semibold())
                        .class(cosmic::theme::Text::Color(COLOR_BRIGHT))
                        .width(Length::Fill),
                );

            let mut form = settings::section();
            form = form.add(
                settings::item::builder(fl!("name")).control(
                    text_input(fl!("name-placeholder"), &self.in_name)
                        .on_input(Message::NameInput)
                        .width(Length::Fill),
                ),
            );
            form = form.add(
                settings::item::builder(fl!("address")).control(
                    text_input(fl!("address-placeholder"), &self.in_address)
                        .on_input(Message::AddressInput)
                        .width(Length::Fill),
                ),
            );
            form = form.add(
                settings::item::builder(fl!("start-command")).control(
                    text_input(fl!("start-command-placeholder"), &self.in_cmd)
                        .on_input(Message::CmdInput)
                        .width(Length::Fill),
                ),
            );
            form = form.add(
                settings::item::builder(fl!("working-directory")).control(
                    text_input(fl!("working-directory-placeholder"), &self.in_dir)
                        .on_input(Message::DirInput)
                        .width(Length::Fill),
                ),
            );
            form = form.add(
                settings::item::builder(fl!("reserve-port"))
                    .description("Hold this port with a silent tcp socket when offline")
                    .control(widget::toggler(self.in_reserved).on_toggle(Message::ReservedInput)),
            );

            let mut form_buttons = widget::row::with_capacity(3)
                .spacing(spacing.space_xs)
                .width(Length::Fill)
                .push(
                    widget::button::standard(fl!("save"))
                        .width(Length::Fill)
                        .on_press(Message::SaveService),
                )
                .push(
                    widget::button::standard(fl!("cancel"))
                        .width(Length::Fill)
                        .on_press(Message::HideAdd),
                );
            if let Some(edit_id) = &self.edit_id {
                form_buttons = form_buttons.push(
                    widget::button::destructive(fl!("delete"))
                        .width(Length::Fill)
                        .on_press(Message::DeleteService(edit_id.clone())),
                );
            }

            if self.industrial() {
                let content = widget::column::with_capacity(5)
                    .spacing(spacing.space_xs)
                    .width(Length::Fill)
                    .push(
                        widget::container(header)
                            .padding([10, 14])
                            .width(Length::Fill)
                            .class(panel_plate(COLOR_PANEL, 0.0)),
                    )
                    .push(seam())
                    .push(
                        widget::container(form)
                            .padding([8, 12])
                            .width(Length::Fill),
                    )
                    .push(
                        widget::container(form_buttons)
                            .padding([8, 14])
                            .width(Length::Fill),
                    );

                let chassis = widget::container(
                    widget::scrollable(content)
                        .class(cosmic::theme::style::iced::Scrollable::Minimal),
                )
                .clip(true)
                .class(chassis_plate())
                .width(Length::Fill)
                .height(Length::Fill);

                return self.core.applet.popup_container(chassis).into();
            }

            let content = widget::column::with_capacity(3)
                .spacing(spacing.space_xs)
                .width(Length::Fill)
                .push(
                    widget::container(header)
                        .padding([10, 14])
                        .width(Length::Fill),
                )
                .push(
                    widget::container(form)
                        .padding([8, 12])
                        .width(Length::Fill),
                )
                .push(
                    widget::container(form_buttons)
                        .padding([8, 14])
                        .width(Length::Fill),
                );

            let scroll = widget::scrollable(
                widget::container(content)
                    .padding([10, 12])
                    .width(Length::Fill),
            );
            return self.core.applet.popup_container(scroll).into();
        }

        // 4. Main view — Industrial Precision instrument chassis
        let mut root = widget::column::with_capacity(16).spacing(6.0);
        let filter_query = self.filter.trim().to_lowercase();
        let matches_filter = |name: &str, needle: &str| -> bool {
            needle.is_empty() || name.to_lowercase().contains(needle)
        };

        // Header: ringed magnifier + borderless filter + flat settings key +
        // title. No brand tile — panel popups don't carry logos, and a chip
        // that looks like a button but isn't is a dead affordance. The search
        // chrome lives on the magnifier ring (the hold-in spot), not on a
        // full-width plate; there is no placeholder text.
        let filter_field = text_input("", &self.filter)
            .on_input(Message::FilterInput)
            .size(12.0)
            .style(filter_input_class())
            .width(Length::Fill);

        let search_plate = widget::row::with_capacity(2)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(8.0)
            .push(
                widget::container(tinted_icon("edit-find-symbolic", 14, COLOR_TEXT2))
                    .width(Length::Fixed(26.0))
                    .height(Length::Fixed(26.0))
                    .align_x(cosmic::iced::Alignment::Center)
                    .align_y(cosmic::iced::Alignment::Center)
                    .class(search_ring()),
            )
            .push(filter_field);

        let header = widget::row::with_capacity(4)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(spacing.space_xs)
            .width(Length::Fill)
            .push(search_plate)
            .push(widget::Space::new().width(Length::Fill))
            .push(
                widget::Text::new(fl!("app-title"))
                    .size(15.0)
                    .font(cosmic::font::semibold())
                    .class(if self.industrial() {
                        cosmic::theme::Text::Color(COLOR_BRIGHT)
                    } else {
                        cosmic::theme::Text::Default
                    }),
            )
            .push(flat_key_button(
                "emblem-system-symbolic",
                "Lazy Ports settings",
                Message::ToggleSettings,
            ));

        if self.industrial() {
            root = root.push(
                widget::container(header)
                    .padding([10, 14])
                    .width(Length::Fill)
                    .class(panel_plate(COLOR_PANEL, 0.0)),
            );
            root = root.push(seam());
        } else {
            root = root.push(
                widget::container(header)
                    .padding([10, 14])
                    .width(Length::Fill),
            );
        }

        if let Some(error) = &self.error {
            root = root.push(
                widget::Text::new(error.clone())
                    .size(11.0)
                    .font(cosmic::font::mono())
                    .class(cosmic::theme::Text::Color(LED_AMBER)),
            );
        }
        if self.services_corrupt {
            root = root.push(
                widget::button::standard(String::from("Reset config"))
                    .on_press(Message::ResetCorruptConfig),
            );
        }

        // Partition pinned services into active (running/starting) and inactive (stopped/failed)
        let (active_services, inactive_services): (Vec<&ServiceConfig>, Vec<&ServiceConfig>) = self
            .services
            .iter()
            .filter(|s| matches_filter(&format!("{} {}", s.name, s.address), &filter_query))
            .partition(|s| {
                let (st, _) = self.service_status(s, &self.ports);
                matches!(st, SvcState::Running | SvcState::Starting)
            });

        // 1. Active services
        if !active_services.is_empty() {
            for cfg in &active_services {
                root = root.push(self.render_service_card(cfg));
            }
        } else if self.services.is_empty() {
            root = root.push(self.empty_state(fl!("no-services")));
        } else if filter_query.is_empty() {
            root = root.push(self.empty_state(fl!("no-active-services")));
        } else {
            root = root.push(self.empty_state("NO ACTIVE SOCKETS MATCHING FILTER".to_string()));
        }

        // 2. Inactive services rail
        if !inactive_services.is_empty() {
            root = root.push(self.section_rail(
                LED_PURPLE,
                &format!("{} ({})", fl!("inactive-services"), inactive_services.len()),
                self.show_inactive,
                Message::ToggleShowInactive,
            ));
            if self.show_inactive {
                for cfg in &inactive_services {
                    root = root.push(self.render_inactive_service_row(cfg));
                }
            }
        }

        // 3. Discovered ports rail
        let ignored = self.config.ignored_processes();
        let pinned_ports: HashSet<u16> = self
            .services
            .iter()
            .filter_map(|s| s.host_port().map(|(_, p)| p))
            .collect();
        let unpinned: Vec<&ports::ListeningPort> = self
            .ports
            .iter()
            .filter(|p| !pinned_ports.contains(&p.port))
            .filter(|p| !ignored.contains(&p.process))
            .filter(|p| !services::is_port_reserved(p.port))
            .filter(|p| !self.config.hide_system || !p.is_system)
            .filter(|p| matches_filter(&format!("{} {} {}", p.display_name, p.process, p.port), &filter_query))
            .collect();

        let unpinned_count = unpinned.len();

        root = root.push(self.section_rail(
            LED_CYAN,
            &format!("{} ({})", fl!("discovered-ports"), unpinned_count),
            self.config.show_ports,
            Message::ToggleShowPorts,
        ));

        if self.config.show_ports {
            if unpinned.is_empty() {
                root = root.push(self.empty_state(fl!("no-ports")));
            } else if self.group_ports {
                let groups = ports::group_by_process_refs(&unpinned);
                for group in groups.iter().take(15) {
                    let port_nums: Vec<u16> = group.ports.iter().map(|p| p.port).collect();
                    let first_port = group.ports.first().copied().cloned();
                    root = root.push(self.render_compact_discovered_group_row(
                        format!("group-{}-{}", group.process, group.display_name),
                        group.display_name.clone(),
                        group.process.clone(),
                        group.framework.clone(),
                        port_nums,
                        group.pids.clone(),
                        first_port,
                    ));
                }
            } else {
                for p in unpinned.iter().take(20) {
                    root = root.push(self.render_compact_discovered_port_row(p));
                }
            }
        }

        // Footer: Rescan + Bind (left) · Quit (right). Refresh-rate selection
        // lives in the settings view; the footer expansion read as a mystery
        // window, not a dropdown.
        let footer = widget::row::with_capacity(4)
            .spacing(spacing.space_xxs)
            .align_y(cosmic::iced::Alignment::Center)
            .push(key_glyph_text_button(
                Some("view-refresh-symbolic"),
                "Rescan",
                Message::Tick,
            ))
            .push(key_glyph_text_button(
                Some("list-add-symbolic"),
                "Add Service",
                Message::ShowAdd,
            ))
            .push(widget::Space::new().width(Length::Fill))
            .push(key_text_button("Quit", Message::QuitApplet));
        if self.industrial() {
            root = root.push(seam());
            root = root.push(
                widget::container(footer)
                    .padding([10, 14])
                    .width(Length::Fill)
                    .class(panel_plate(COLOR_FOOTER, 0.0)),
            );
        } else {
            root = root.push(
                widget::container(footer)
                    .padding([10, 14])
                    .width(Length::Fill),
            );
        }

        // Industrial keeps the dark instrument chassis; system mode lets the
        // COSMIC popup background (via `applet::style()`) show through.
        if self.industrial() {
            let body = widget::container(root).padding([10, 12]).width(Length::Fill);

            let chassis = widget::container(
                widget::scrollable(body).class(cosmic::theme::style::iced::Scrollable::Minimal),
            )
            .clip(true)
            .class(chassis_plate())
            .width(Length::Fill)
            .height(Length::Fill);

            return self.core.applet.popup_container(chassis).into();
        }

        let body = widget::container(root)
            .padding([10, 12])
            .width(Length::Fill);
        let scroll = widget::scrollable(body);
        self.core.applet.popup_container(scroll).into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let subs = vec![
            Subscription::run(|| {
                cosmic::iced::stream::channel(
                    16,
                    |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                        let (tx, mut rx) = futures::channel::mpsc::unbounded::<services::Event>();
                        let _ = services::set_event_tx(tx);
                        while let Some(ev) = rx.next().await {
                            _ = output.send(Message::Event(ev)).await;
                        }
                    },
                )
            }),
            Subscription::run_with(
                (self.popup.is_some(), self.config.refresh_interval()),
                |k| {
                    let (open, secs) = *k;
                    cosmic::iced::stream::channel(
                        4,
                        move |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                            if !open {
                                cosmic::iced::futures::future::pending::<()>().await;
                            }
                            let mut interval = tokio::time::interval(Duration::from_secs(secs));
                            interval
                                .set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                            loop {
                                interval.tick().await;
                                _ = output.send(Message::Tick).await;
                            }
                        },
                    )
                },
            ),
            self.core()
                .watch_config::<Config>(APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
        ];

        Subscription::batch(subs)
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    self.show_inactive = false;
                    self.logs_open = None;
                    // Defer the destroy until after the pointer is released.
                    // cosmic-comp holds an active pointer grab while the
                    // applet button is pressed, and destroying the grabbed
                    // popup inside that press leaves an orphaned seat grab
                    // that silently swallows every later click.
                    self.popup_closing = Some(p);
                    Task::perform(
                        async {
                            tokio::time::sleep(Duration::from_millis(120)).await;
                        },
                        move |_| action(Message::DestroyPopup(p)),
                    )
                } else if self.popup_closing.is_some() {
                    Task::none()
                } else {
                    self.show_inactive = false;
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(476.0)
                        .min_width(476.0)
                        .min_height(200.0)
                        .max_height(1080.0);
                    get_popup(popup_settings)
                };
            }
            Message::DestroyPopup(id) => {
                if self.popup_closing == Some(id) {
                    self.popup_closing = None;
                }
                return destroy_popup(id);
            }
            Message::PopupClosed(id) => {
                if self.popup_closing == Some(id) {
                    self.popup_closing = None;
                }
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                    self.show_inactive = false;
                    self.logs_open = None;
                }
            }
            Message::Tick => {
                let svcs = self.services.clone();
                let task = Task::batch(vec![
                    Task::perform(ports::scan(), |v| action(Message::PortsRefreshed(v))),
                    Task::perform(services::health_all(svcs), |v| {
                        action(Message::HealthBatch(v))
                    }),
                ]);
                if let Some(log_id) = self.logs_open.clone() {
                    let path = services::log_file_path(&log_id)
                        .to_string_lossy()
                        .into_owned();
                    return Task::batch(vec![
                        task,
                        Task::perform(services::read_tail(path), move |t| {
                            action(Message::LogText {
                                id: log_id.clone(),
                                text: t,
                            })
                        }),
                    ]);
                }
                return task;
            }
            Message::PortsRefreshed(list) => self.ports = list,
            Message::HealthBatch(results) => {
                for (id, health) in results {
                    self.health.insert(id, health);
                }
            }
            Message::StartService(id) => {
                if let Some(cfg) = self.services.iter().find(|s| s.id == id).cloned() {
                    self.states.insert(id.clone(), SvcState::Starting);
                    return Task::perform(services::spawn_managed(cfg), move |result| {
                        action(Message::StartResult(id.clone(), result))
                    });
                }
            }
            Message::StartResult(id, result) => match result {
                Ok(_) => {
                    if services::is_managed(&id) {
                        self.states.insert(id, SvcState::Running);
                        self.error = None;
                    }
                    services::sync_reservations(&self.services);
                }
                Err(e) => {
                    self.states.insert(id.clone(), SvcState::Failed(e.clone()));
                    self.error = Some(e);
                    services::sync_reservations(&self.services);
                }
            },
            Message::ToggleGroup => {
                self.group_ports = !self.group_ports;
            }
            Message::ToggleSettings => {
                self.show_settings = !self.show_settings;
            }
            Message::ToggleShowPorts => {
                self.config.show_ports = !self.config.show_ports;
                if let Some(ctx) = &self.config_ctx {
                    let _ = ctx.set("show_ports", self.config.show_ports);
                }
            }
            Message::ToggleShowInactive => {
                self.show_inactive = !self.show_inactive;
            }
            Message::ToggleExpandService(id) => {
                if self.expanded_service.as_deref() == Some(&id) {
                    self.expanded_service = None;
                } else {
                    self.expanded_service = Some(id);
                }
            }
            Message::ToggleReservePort(id) => {
                if let Some(pos) = self.services.iter().position(|s| s.id == id) {
                    self.services[pos].reserved = !self.services[pos].reserved;
                    if let Err(e) = self.persist_services() {
                        self.error = Some(e);
                    }
                    services::sync_reservations(&self.services);
                }
            }
            Message::KillProcessGroup(pids) => {
                let tasks: Vec<_> = pids
                    .into_iter()
                    .map(|pid| Task::perform(services::kill_pid(pid), |_| action(Message::Tick)))
                    .collect();
                return Task::batch(tasks);
            }
            Message::IgnorePort(p) => {
                let mut ignored = self.config.ignored_processes();
                if !ignored.contains(&p.process) {
                    ignored.push(p.process.clone());
                    let json = serde_json::to_string(&ignored).unwrap_or_default();
                    self.config.ignored_json = json.clone();
                    if let Some(ctx) = &self.config_ctx {
                        let _ = ctx.set("ignored_json", json);
                    }
                }
            }
            Message::ClearIgnored => {
                self.config.ignored_json = "[]".to_string();
                if let Some(ctx) = &self.config_ctx {
                    let _ = ctx.set("ignored_json", "[]".to_string());
                }
            }
            Message::RestartService(id) => {
                if let Some(cfg) = self.services.iter().find(|s| s.id == id).cloned() {
                    self.states.insert(id.clone(), SvcState::Starting);
                    self.stopping.insert(id.clone());
                    return Task::perform(services::restart_managed(cfg), move |result| {
                        action(Message::RestartResult(id.clone(), result))
                    });
                }
            }
            Message::RestartResult(id, result) => match result {
                Ok((_pgid, stopped_old)) => {
                    if services::is_managed(&id) {
                        self.states.insert(id.clone(), SvcState::Running);
                        self.error = None;
                    }
                    if !stopped_old {
                        self.stopping.remove(&id);
                    }
                    services::sync_reservations(&self.services);
                }
                Err(e) => {
                    self.states.insert(id.clone(), SvcState::Failed(e.clone()));
                    self.error = Some(e);
                    self.stopping.remove(&id);
                    services::sync_reservations(&self.services);
                }
            },
            Message::StopService(id) => {
                self.stopping.insert(id.clone());
                return Task::perform(services::stop_managed(id.clone()), move |_| {
                    action(Message::StopResult(id))
                });
            }
            Message::StopResult(id) => {
                self.stopping.remove(&id);
                self.states.insert(id, SvcState::Stopped);
                services::sync_reservations(&self.services);
            }
            Message::Exited(id, code) => {
                let crashed = !self.stopping.remove(&id)
                    && code != Some(0)
                    && matches!(
                        self.states.get(&id),
                        Some(SvcState::Running | SvcState::Starting)
                    );
                if !matches!(self.states.get(&id), Some(SvcState::Failed(_))) {
                    self.states.insert(id.clone(), SvcState::Stopped);
                }
                services::sync_reservations(&self.services);
                if crashed && let Some(cfg) = self.services.iter().find(|s| s.id == id).cloned() {
                    return Task::perform(services::notify_crash(cfg.name, code), |_| {
                        action(Message::Tick)
                    });
                }
            }
            Message::Event(services::Event::Exited { id, code }) => {
                return self.update(Message::Exited(id, code));
            }
            Message::Open(url) => {
                if let Err(e) = open::that_detached(&url) {
                    self.error = Some(format!("failed to open {url}: {e}"));
                }
            }
            Message::QuitApplet => {
                std::process::exit(0);
            }
            Message::KillPort(pid) => {
                return Task::perform(services::kill_pid(pid), move |_| action(Message::Tick));
            }
            Message::PinPort(p) => {
                self.show_add = true;
                self.edit_id = None;
                self.in_name = p.process.clone();
                self.in_address = format!("http://localhost:{}", p.port);
                self.in_cmd = ports::process_cmdline(p.pid).unwrap_or_default();
                self.in_dir = ports::process_cwd(p.pid).unwrap_or_default();
                self.in_reserved = false;
            }
            Message::ShowAdd => {
                self.show_add = true;
                self.edit_id = None;
                self.in_name.clear();
                self.in_address.clear();
                self.in_cmd.clear();
                self.in_dir.clear();
                self.in_reserved = false;
            }
            Message::HideAdd => {
                self.show_add = false;
                self.edit_id = None;
                self.in_reserved = false;
            }
            Message::ToggleHideSystem => {
                self.config.hide_system = !self.config.hide_system;
                if let Some(ctx) = &self.config_ctx {
                    let _ = ctx.set("hide_system", self.config.hide_system);
                }
            }
            Message::ToggleColloquial => {
                self.config.colloquial = !self.config.colloquial;
                if let Some(ctx) = &self.config_ctx {
                    let _ = ctx.set("colloquial", self.config.colloquial);
                }
            }
            Message::SetRefresh(secs) => {
                self.config.refresh_secs = secs;
                if let Some(ctx) = &self.config_ctx {
                    let _ = ctx.set("refresh_secs", secs);
                }
            }
            Message::SetThemeMode(name) => {
                let normalised = if name.trim().eq_ignore_ascii_case("industrial") {
                    "industrial".to_string()
                } else {
                    "system".to_string()
                };
                self.config.theme_mode = normalised;
                if let Some(ctx) = &self.config_ctx {
                    let _ = ctx.set("theme_mode", self.config.theme_mode.clone());
                }
            }
            Message::FilterInput(v) => self.filter = v,
            Message::FilterTo(v) => self.filter = v,
            Message::HoverRow(id) => self.hover = id,
            Message::NameInput(v) => self.in_name = v,
            Message::AddressInput(v) => self.in_address = v,
            Message::CmdInput(v) => self.in_cmd = v,
            Message::DirInput(v) => self.in_dir = v,
            Message::ReservedInput(v) => self.in_reserved = v,
            Message::SaveService => {
                if let Err(err) = self.save_service() {
                    self.error = Some(err);
                }
            }
            Message::DeleteService(id) => {
                self.services.retain(|s| s.id != id);
                self.states.remove(&id);
                self.health.remove(&id);
                if self.logs_open.as_deref() == Some(id.as_str()) {
                    self.logs_open = None;
                }
                if let Err(err) = self.persist_services() {
                    self.error = Some(err);
                }
                services::sync_reservations(&self.services);
                return Task::perform(services::stop_managed(id), |_| action(Message::Tick));
            }
            Message::MoveServiceUp(id) => {
                if let Some(pos) = self.services.iter().position(|s| s.id == id) {
                    if pos > 0 {
                        self.services.swap(pos, pos - 1);
                        if let Err(err) = self.persist_services() {
                            self.error = Some(err);
                        }
                    }
                }
            }
            Message::MoveServiceDown(id) => {
                if let Some(pos) = self.services.iter().position(|s| s.id == id) {
                    if pos + 1 < self.services.len() {
                        self.services.swap(pos, pos + 1);
                        if let Err(err) = self.persist_services() {
                            self.error = Some(err);
                        }
                    }
                }
            }
            Message::ResetCorruptConfig => {
                self.services_corrupt = false;
                self.error = None;
                if let Err(err) = self.persist_services() {
                    self.error = Some(err);
                }
                services::sync_reservations(&self.services);
            }
            Message::EditService(id) => {
                if let Some(cfg) = self.services.iter().find(|s| s.id == id).cloned() {
                    self.show_add = true;
                    self.edit_id = Some(cfg.id.clone());
                    self.in_name = cfg.name.clone();
                    self.in_address = cfg.address.clone();
                    self.in_cmd = cfg.start_command.clone().unwrap_or_default();
                    self.in_dir = cfg.working_dir.clone().unwrap_or_default();
                    self.in_reserved = cfg.reserved;
                }
            }
            Message::ShowLogs(id) => {
                self.logs_open = Some(id.clone());
                let path = services::log_file_path(&id).to_string_lossy().into_owned();
                return Task::perform(services::read_tail(path), move |t| {
                    action(Message::LogText {
                        id: id.clone(),
                        text: t,
                    })
                });
            }
            Message::HideLogs => {
                self.logs_open = None;
                self.log_text.clear();
            }
            Message::LogText { id, text } => {
                if self.logs_open.as_deref() == Some(&id) {
                    self.log_text = text;
                }
            }
            Message::UpdateConfig(config) => {
                match parse_services(&config.services_json) {
                    Ok(services) => {
                        self.services = services;
                        self.services_corrupt = false;
                    }
                    Err(_) => {
                        self.services = Vec::new();
                        self.services_corrupt = true;
                        self.error = Some(CORRUPT_CONFIG_MSG.to_string());
                    }
                }
                self.config = config;
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

impl AppModel {
    /// Centered mono empty state on the viewport plate.
    fn empty_state<'a>(&'a self, msg: String) -> Element<'a, Message> {
        if self.industrial() {
            return widget::container(
                widget::Text::new(msg)
                    .size(12.0)
                    .font(cosmic::font::mono())
                    .class(cosmic::theme::Text::Color(COLOR_SUBTLE)),
            )
            .padding(20)
            .width(Length::Fill)
            .center_x(Length::Fill)
            .class(panel_plate(COLOR_VIEWPORT, 4.0))
            .into();
        }
        widget::container(
            widget::Text::new(msg)
                .size(12.0)
                .font(cosmic::font::mono()),
        )
        .padding(20)
        .width(Length::Fill)
        .center_x(Length::Fill)
        .class(cosmic::theme::Container::Secondary)
        .into()
    }

    /// Compact collapsible section rail: LED · title · show/hide key.
    fn section_rail(
        &self,
        led: cosmic::iced::Color,
        title: &str,
        open: bool,
        toggle: Message,
    ) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        if self.industrial() {
            return widget::container(
                widget::row::with_capacity(3)
                    .align_y(cosmic::iced::Alignment::Center)
                    .spacing(spacing.space_xxs)
                    .push(status_led(led))
                    .push(
                        widget::Text::new(title.to_string())
                            .size(12.0)
                            .font(cosmic::font::semibold())
                            .class(cosmic::theme::Text::Color(COLOR_BRIGHT))
                            .width(Length::Fill),
                    )
                    .push(key_text_button(
                        if open { fl!("hide") } else { fl!("show") },
                        toggle,
                    )),
            )
            .padding([6, 12])
            .width(Length::Fill)
            .class(panel_plate(COLOR_PANEL, 0.0))
            .into();
        }
        widget::container(
            widget::row::with_capacity(3)
                .align_y(cosmic::iced::Alignment::Center)
                .spacing(spacing.space_xxs)
                .push(status_led(led))
                .push(
                    widget::Text::new(title.to_string())
                        .size(12.0)
                        .font(cosmic::font::semibold())
                        .width(Length::Fill),
                )
                .push(key_text_button(
                    if open { fl!("hide") } else { fl!("show") },
                    toggle,
                )),
        )
        .padding([6, 12])
        .width(Length::Fill)
        .into()
    }

    fn save_service(&mut self) -> Result<(), String> {
        let name = self.in_name.trim().to_string();
        if name.is_empty() {
            return Err(fl!("error-name-required"));
        }

        let raw = self.in_address.trim().to_string();
        let normalized = if raw.is_empty() {
            return Err(fl!("error-invalid-address"));
        } else if raw.contains("://") {
            raw
        } else {
            format!("http://{raw}")
        };

        let Some((_, port, _, _)) = parse_address(&normalized) else {
            return Err(fl!("error-missing-port"));
        };

        let editing_other = |s: &ServiceConfig| Some(&s.id) != self.edit_id.as_ref();
        if let Some(other) = self
            .services
            .iter()
            .filter(|s| editing_other(s))
            .find(|s| s.host_port().map(|(_, p)| p) == Some(port))
        {
            return Err(fl!(
                "error-duplicate-port",
                port = format!("{port} ({})", other.name)
            ));
        }

        let cfg = ServiceConfig {
            id: self
                .edit_id
                .clone()
                .unwrap_or_else(|| format!("service-{port}")),
            name,
            address: normalized,
            start_command: opt_str(&self.in_cmd),
            working_dir: opt_str(&self.in_dir),
            reserved: self.in_reserved,
        };

        if let Some(edit_id) = &self.edit_id {
            if let Some(pos) = self.services.iter().position(|s| &s.id == edit_id) {
                self.services[pos] = cfg;
            } else {
                self.services.push(cfg);
            }
        } else {
            self.services.push(cfg);
        }

        self.persist_services()?;
        services::sync_reservations(&self.services);
        self.show_add = false;
        self.edit_id = None;
        self.in_name.clear();
        self.in_address.clear();
        self.in_cmd.clear();
        self.in_dir.clear();
        self.in_reserved = false;
        Ok(())
    }

    /// Finds the listening port matching a service's configured address port.
    fn matching_port<'a>(
        &self,
        cfg: &ServiceConfig,
        ports: &'a [ports::ListeningPort],
    ) -> Option<&'a ports::ListeningPort> {
        let (_, port) = cfg.host_port()?;
        ports.iter().find(|p| p.port == port)
    }

    /// Determines the effective visual state of a service:
    /// - Starting: if tracked as Starting in states map
    /// - Running: if tracked as Running in states map, or if matching port is listening
    /// - Failed: if tracked as Failed in states map
    /// - Stopped: default
    fn service_status<'a>(
        &self,
        cfg: &ServiceConfig,
        ports: &'a [ports::ListeningPort],
    ) -> (SvcState, Option<&'a ports::ListeningPort>) {
        let port_num = cfg.host_port().map(|(_, p)| p).unwrap_or(0);
        let is_reserved = cfg.reserved && services::is_port_reserved(port_num);

        if let Some(state) = self.states.get(&cfg.id) {
            if matches!(state, SvcState::Starting) {
                return (SvcState::Starting, self.matching_port(cfg, ports));
            }
            if matches!(state, SvcState::Running) {
                return (SvcState::Running, self.matching_port(cfg, ports));
            }
        }
        if !is_reserved && let Some(lp) = self.matching_port(cfg, ports) {
            return (SvcState::Running, Some(lp));
        }
        if let Some(state) = self.states.get(&cfg.id)
            && matches!(state, SvcState::Failed(_))
        {
            return (state.clone(), None);
        }
        (SvcState::Stopped, None)
    }

    fn persist_services(&mut self) -> Result<(), String> {
        if self.services_corrupt {
            return Err(CORRUPT_CONFIG_MSG.to_string());
        }
        let json = serde_json::to_string(&self.services).unwrap_or_default();
        self.config.services_json = json.clone();
        match &self.config_ctx {
            Some(ctx) => {
                ctx.set("services_json", json)
                    .map_err(|e| format!("config write failed: {e}"))?;
            }
            None => return Err("config context unavailable".to_string()),
        }
        Ok(())
    }

    fn render_service_drawer<'a>(
        &'a self,
        cfg: &'a ServiceConfig,
        is_running: bool,
    ) -> Option<Element<'a, Message>> {
        let spacing = cosmic::theme::spacing();
        let mut info_col = widget::column::with_capacity(3)
            .spacing(spacing.space_xxxs)
            .width(Length::Fill);

        if let Some(cmd) = &cfg.start_command {
            info_col = info_col.push(
                widget::row::with_capacity(2)
                    .spacing(spacing.space_xxs)
                    .width(Length::Fill)
                    .align_y(cosmic::iced::Alignment::Center)
                    .push(faint_mono("CMD"))
                    .push(
                        widget::container(dim_mono(cmd.clone()))
                            .width(Length::Fill)
                            .clip(true),
                    ),
            );
        }
        if let Some(dir) = &cfg.working_dir {
            info_col = info_col.push(
                widget::row::with_capacity(2)
                    .spacing(spacing.space_xxs)
                    .width(Length::Fill)
                    .align_y(cosmic::iced::Alignment::Center)
                    .push(faint_mono("DIR"))
                    .push(
                        widget::container(dim_mono(dir.clone()))
                            .width(Length::Fill)
                            .clip(true),
                    ),
            );
        }
        if cfg.start_command.is_none() && cfg.working_dir.is_none() {
            info_col = info_col.push(dim_mono("No start command configured"));
        }

        let lock_color = if cfg.reserved {
            KeyColor::Blue
        } else {
            KeyColor::Neutral
        };
        let lock_tip = if cfg.reserved {
            "Port reserved (click to unlock)"
        } else {
            "Reserve port (hold when stopped)"
        };

        // Compact toolbar of milled keys
        let mut action_row = widget::row::with_capacity(7)
            .spacing(6.0)
            .align_y(cosmic::iced::Alignment::Center);

        // Move Up / Move Down for reordering
        action_row = action_row.push(key_button(
            "go-up-symbolic",
            "Move up in list",
            self.accent(),
            Message::MoveServiceUp(cfg.id.clone()),
            KeyColor::Neutral,
        ));
        action_row = action_row.push(key_button(
            "go-down-symbolic",
            "Move down in list",
            self.accent(),
            Message::MoveServiceDown(cfg.id.clone()),
            KeyColor::Neutral,
        ));

        if is_running && services::is_managed(&cfg.id) {
            action_row = action_row.push(key_button(
                "view-refresh-symbolic",
                "Restart process",
                self.accent(),
                Message::RestartService(cfg.id.clone()),
                KeyColor::Blue,
            ));
        }

        action_row = action_row.push(key_button(
            "utilities-terminal-symbolic",
            "View live logs",
            self.accent(),
            Message::ShowLogs(cfg.id.clone()),
            KeyColor::Neutral,
        ));

        action_row = action_row.push(key_button(
            "changes-prevent-symbolic",
            lock_tip,
            self.accent(),
            Message::ToggleReservePort(cfg.id.clone()),
            lock_color,
        ));

        action_row = action_row.push(key_button(
            "document-edit-symbolic",
            "Edit service",
            self.accent(),
            Message::EditService(cfg.id.clone()),
            KeyColor::Neutral,
        ));

        action_row = action_row.push(widget::Space::new().width(Length::Fill));

        action_row = action_row.push(key_button(
            "user-trash-symbolic",
            "Delete / Unpin service",
            self.accent(),
            Message::DeleteService(cfg.id.clone()),
            KeyColor::Danger,
        ));

        let drawer_col = widget::column::with_capacity(3)
            .spacing(6.0)
            .width(Length::Fill)
            .push(info_col)
            .push(action_row);

        Some(
            widget::container(drawer_col)
                .width(Length::Fill)
                .padding([6, 10])
                .class(drawer_plate())
                .into(),
        )
    }

    fn render_service_card<'a>(&'a self, cfg: &'a ServiceConfig) -> Element<'a, Message> {
        let (state, matching_lp) = self.service_status(cfg, &self.ports);
        let is_expanded = self.expanded_service.as_deref() == Some(&cfg.id);
        let port_num = cfg.host_port().map(|(_, p)| p).unwrap_or(0);
        let is_reserved = cfg.reserved && services::is_port_reserved(port_num);
        let is_running = matches!(state, SvcState::Running | SvcState::Starting);

        // Status LED and port color: Green for Active, Yellow/Amber for Failed/Crashed
        let (led_color, port_color) = match &state {
            SvcState::Running | SvcState::Starting => (LED_GREEN, LED_GREEN),
            SvcState::Failed(_) => (LED_AMBER, LED_AMBER),
            SvcState::Stopped => {
                if is_reserved {
                    (LED_CYAN, LED_CYAN)
                } else {
                    (LED_PURPLE, LED_PURPLE)
                }
            }
        };

        // Process tag for the badge chip.
        let badge_label = if let Some(lp) = matching_lp {
            lp.process.clone()
        } else if let Some(cmd) = &cfg.start_command {
            cmd.split_whitespace()
                .next()
                .unwrap_or("PINNED")
                .split('/')
                .next_back()
                .unwrap_or("PINNED")
                .to_string()
        } else {
            "PINNED".to_string()
        };

        let pid_or_state: String = match &state {
            SvcState::Running => match matching_lp {
                Some(lp) => format!("PID {}", lp.pid),
                None => self.lingo("RUNNING".to_string(), "PIMPIN'"),
            },
            SvcState::Starting => self.lingo("STARTING…".to_string(), "WAKIN' UP"),
            SvcState::Stopped => {
                if is_reserved {
                    "RESERVED".to_string()
                } else {
                    self.lingo("STOPPED".to_string(), "SLEEPIN'")
                }
            }
            SvcState::Failed(e) => {
                let short: String = e.chars().take(8).collect();
                let dots = if e.chars().count() > 8 { "…" } else { "" };
                self.lingo(format!("BROKE: {short}{dots}"), &format!("COOKED: {short}{dots}"))
            }
        };

        let endpoint = if port_num > 0 {
            endpoint_line(
                "localhost",
                port_num.to_string(),
                pid_or_state,
                port_color,
            )
        } else {
            dim_mono(pid_or_state)
        };

        let title = link_title(cfg.name.clone(), cfg.address.clone());

        let pin_btn = Some(pin_circle_button(
            Message::EditService(cfg.id.clone()),
            true,
            self.accent(),
        ));

        // Action keys: start-stop | expand
        let mut actions = widget::row::with_capacity(2).spacing(6.0);
        if is_running {
            let stop_msg = if services::is_managed(&cfg.id) {
                Message::StopService(cfg.id.clone())
            } else if let Some(lp) = matching_lp {
                Message::KillPort(lp.pid)
            } else {
                Message::StopService(cfg.id.clone())
            };
            actions = actions.push(key_button(
                "media-playback-stop-symbolic",
                "Stop process",
                self.accent(),
                stop_msg,
                KeyColor::Danger,
            ));
        } else if cfg.can_start() {
            actions = actions.push(key_button(
                "media-playback-start-symbolic",
                "Start service",
                self.accent(),
                Message::StartService(cfg.id.clone()),
                KeyColor::Green,
            ));
        }
        let expand_icon = if is_expanded {
            "go-up-symbolic"
        } else {
            "go-down-symbolic"
        };
        actions = actions.push(key_button(
            expand_icon,
            "Expand / collapse service details",
            self.accent(),
            Message::ToggleExpandService(cfg.id.clone()),
            KeyColor::Neutral,
        ));

        let badge = filter_badge(badge_label.clone(), badge_label);

        let drawer = if is_expanded {
            self.render_service_drawer(cfg, is_running)
        } else {
            None
        };

        card_shell(
            led_color,
            title,
            badge,
            pin_btn,
            endpoint,
            actions.into(),
            drawer,
            self.hover.as_deref() == Some(cfg.id.as_str()),
            cfg.id.clone(),
            self.industrial(),
        )
    }

    fn render_inactive_service_row<'a>(&'a self, cfg: &'a ServiceConfig) -> Element<'a, Message> {
        let industrial = self.industrial();
        let is_expanded = self.expanded_service.as_deref() == Some(&cfg.id);
        let (state, _) = self.service_status(cfg, &self.ports);
        let port_num = cfg.host_port().map(|(_, p)| p).unwrap_or(0);
        let is_reserved = cfg.reserved && services::is_port_reserved(port_num);
        let led_color = match &state {
            SvcState::Failed(_) => LED_AMBER,
            SvcState::Running | SvcState::Starting => LED_GREEN,
            SvcState::Stopped => {
                if is_reserved {
                    LED_CYAN
                } else {
                    LED_PURPLE
                }
            }
        };

        let title = link_title(cfg.name.clone(), cfg.address.clone());

        let badge: Element<'a, Message> = if is_reserved {
            themed_badge("LOCKED".to_string(), industrial)
        } else {
            widget::Space::new().width(Length::Shrink).into()
        };

        let state_label = if is_reserved {
            "RESERVED".to_string()
        } else {
            self.lingo("STOPPED".to_string(), "SLEEPIN'")
        };

        let port_label = if port_num > 0 {
            format!(":{port_num}")
        } else {
            cfg.address.clone()
        };

        let endpoint = themed_endpoint(port_label, state_label, LED_PURPLE, industrial);

        let mut actions = widget::row::with_capacity(2).spacing(6.0);
        if cfg.can_start() {
            actions = actions.push(key_button(
                "media-playback-start-symbolic",
                "Start service",
                self.accent(),
                Message::StartService(cfg.id.clone()),
                KeyColor::Green,
            ));
        }

        let expand_icon = if is_expanded {
            "go-up-symbolic"
        } else {
            "go-down-symbolic"
        };
        actions = actions.push(key_button(
            expand_icon,
            "Service details & actions",
            self.accent(),
            Message::ToggleExpandService(cfg.id.clone()),
            KeyColor::Neutral,
        ));

        let pin_btn = Some(pin_circle_button(
            Message::EditService(cfg.id.clone()),
            true,
            self.accent(),
        ));

        let drawer = if is_expanded {
            self.render_service_drawer(cfg, false)
        } else {
            None
        };

        card_shell(
            led_color,
            title,
            badge,
            pin_btn,
            endpoint,
            actions.into(),
            drawer,
            self.hover.as_deref() == Some(cfg.id.as_str()),
            cfg.id.clone(),
            industrial,
        )
    }

    fn render_compact_discovered_port_row<'a>(
        &'a self,
        p: &'a ports::ListeningPort,
    ) -> Element<'a, Message> {
        let spacing = cosmic::theme::spacing();
        let industrial = self.industrial();
        let row_id = format!("port-{}", p.port);
        let is_expanded = self.expanded_service.as_deref() == Some(&row_id);
        let port_url = format!("http://localhost:{}", p.port);

        let title = plain_title(p.display_name.clone(), industrial);

        let badge_label = if let Some(fw) = &p.framework {
            fw.clone()
        } else if p.process != p.display_name {
            p.process.clone()
        } else {
            String::new()
        };
        let badge: Element<'a, Message> = if badge_label.is_empty() {
            widget::Space::new().width(Length::Shrink).into()
        } else {
            themed_badge(badge_label, industrial)
        };

        let endpoint = themed_endpoint(
            format!(":{}", p.port),
            format!("PID {}", p.pid),
            LED_CYAN,
            industrial,
        );

        let mut actions = widget::row::with_capacity(3).spacing(6.0);
        actions = actions.push(tip(
            widget::button::custom(tinted_icon("external-link-symbolic", 13, COLOR_TEXT2))
                .padding(4)
                .class(cosmic::theme::Button::Custom {
                    active: Box::new(|_, _| flat_btn_style(COLOR_TEXT2)),
                    disabled: Box::new(|_| flat_btn_style(COLOR_SUBTLE)),
                    hovered: Box::new(|_, _| flat_btn_style(LED_CYAN)),
                    pressed: Box::new(|_, _| flat_btn_style(COLOR_TEXT2)),
                })
                .on_press(Message::Open(port_url.clone())),
            format!("Open {port_url}"),
        ));

        actions = actions.push(key_button(
            "media-playback-stop-symbolic",
            "Stop process",
            self.accent(),
            Message::KillPort(p.pid),
            KeyColor::Danger,
        ));

        let expand_icon = if is_expanded {
            "go-up-symbolic"
        } else {
            "go-down-symbolic"
        };
        actions = actions.push(key_button(
            expand_icon,
            "Process details",
            self.accent(),
            Message::ToggleExpandService(row_id.clone()),
            KeyColor::Neutral,
        ));

        let pin_btn = Some(pin_circle_button(
            Message::PinPort(p.clone()),
            false,
            self.accent(),
        ));

        let drawer = if is_expanded {
            let cmd = ports::process_cmdline(p.pid).unwrap_or_else(|| p.process.clone());
            let cwd = ports::process_cwd(p.pid).unwrap_or_default();
            let mut info_col = widget::column::with_capacity(3)
                .spacing(spacing.space_xxxs)
                .width(Length::Fill);

            info_col = info_col.push(
                widget::row::with_capacity(2)
                    .spacing(spacing.space_xxs)
                    .push(faint_mono("BIND"))
                    .push(dim_mono(format!("{}:{}", p.host, p.port))),
            );
            if !cwd.is_empty() {
                info_col = info_col.push(
                    widget::row::with_capacity(2)
                        .spacing(spacing.space_xxs)
                        .push(faint_mono("DIR"))
                        .push(widget::container(dim_mono(cwd)).width(Length::Fill).clip(true)),
                );
            }
            if !cmd.is_empty() {
                info_col = info_col.push(
                    widget::row::with_capacity(2)
                        .spacing(spacing.space_xxs)
                        .push(faint_mono("CMD"))
                        .push(widget::container(dim_mono(cmd)).width(Length::Fill).clip(true)),
                );
            }
            Some(
                widget::container(info_col)
                    .padding([4, 8])
                    .class(drawer_class(industrial))
                    .width(Length::Fill)
                    .into(),
            )
        } else {
            None
        };

        card_shell(
            LED_CYAN,
            title,
            badge,
            pin_btn,
            endpoint,
            actions.into(),
            drawer,
            self.hover.as_deref() == Some(row_id.as_str()),
            row_id,
            industrial,
        )
    }

    fn render_compact_discovered_group_row<'a>(
        &'a self,
        row_id: String,
        display_name: String,
        process_name: String,
        framework: Option<String>,
        port_nums: Vec<u16>,
        pids: Vec<i32>,
        first_port_opt: Option<ports::ListeningPort>,
    ) -> Element<'a, Message> {
        let spacing = cosmic::theme::spacing();
        let industrial = self.industrial();
        let is_expanded = self.expanded_service.as_deref() == Some(&row_id);
        let first_port = port_nums.first().copied().unwrap_or(0);

        // Cap the hero line: full list lives in the drawer. This keeps
        // multi-port groups (":35283 · :57621 +1") on one clipped line
        // instead of overflowing into the action keys.
        const MAX_INLINE_PORTS: usize = 3;
        let shown: Vec<String> = port_nums
            .iter()
            .take(MAX_INLINE_PORTS)
            .map(|p| format!(":{p}"))
            .collect();
        let mut port_label = shown.join(" · ");
        if port_nums.len() > MAX_INLINE_PORTS {
            port_label.push_str(&format!(" +{}", port_nums.len() - MAX_INLINE_PORTS));
        }

        let pid_spec = if pids.len() > 1 {
            format!("{} PIDs", pids.len())
        } else {
            format!("PID {}", pids.first().copied().unwrap_or(0))
        };

        // Title row owns only the (clipped) name + tag. Ports + PID move to
        // the full-width endpoint line below, so overlap is impossible.
        let title = plain_title(display_name.clone(), industrial);

        let badge_label = if let Some(fw) = &framework {
            fw.clone()
        } else if process_name != display_name {
            process_name.clone()
        } else {
            String::new()
        };
        let badge: Element<'a, Message> = if badge_label.is_empty() {
            widget::Space::new().width(Length::Shrink).into()
        } else {
            themed_badge(badge_label, industrial)
        };

        let endpoint = themed_endpoint(port_label, pid_spec, LED_CYAN, industrial);

        let mut actions = widget::row::with_capacity(3).spacing(6.0);
        if first_port > 0 {
            let url = format!("http://localhost:{first_port}");
            actions = actions.push(tip(
                widget::button::custom(tinted_icon("external-link-symbolic", 13, COLOR_TEXT2))
                    .padding(4)
                    .class(cosmic::theme::Button::Custom {
                        active: Box::new(|_, _| flat_btn_style(COLOR_TEXT2)),
                        disabled: Box::new(|_| flat_btn_style(COLOR_SUBTLE)),
                        hovered: Box::new(|_, _| flat_btn_style(LED_CYAN)),
                        pressed: Box::new(|_, _| flat_btn_style(COLOR_TEXT2)),
                    })
                    .on_press(Message::Open(url.clone())),
                format!("Open {url}"),
            ));
        }

        actions = actions.push(key_button(
            "media-playback-stop-symbolic",
            "Stop process group",
            self.accent(),
            Message::KillProcessGroup(pids.clone()),
            KeyColor::Danger,
        ));

        let expand_icon = if is_expanded {
            "go-up-symbolic"
        } else {
            "go-down-symbolic"
        };
        actions = actions.push(key_button(
            expand_icon,
            "Process group details",
            self.accent(),
            Message::ToggleExpandService(row_id.clone()),
            KeyColor::Neutral,
        ));

        let pin_btn = first_port_opt.map(|fp| {
            pin_circle_button(Message::PinPort(fp), false, self.accent())
        });

        let drawer = if is_expanded {
            let mut info_col = widget::column::with_capacity(4)
                .spacing(spacing.space_xxxs)
                .width(Length::Fill);
            info_col = info_col.push(
                widget::row::with_capacity(2)
                    .spacing(spacing.space_xxs)
                    .push(faint_mono("PORTS"))
                    .push(dim_mono(
                        port_nums
                            .iter()
                            .map(|p| p.to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                    )),
            );
            info_col = info_col.push(
                widget::row::with_capacity(2)
                    .spacing(spacing.space_xxs)
                    .push(faint_mono("PIDS"))
                    .push(dim_mono(
                        pids.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", "),
                    )),
            );
            Some(
                widget::container(info_col)
                    .padding([4, 8])
                    .class(drawer_class(industrial))
                    .width(Length::Fill)
                    .into(),
            )
        } else {
            None
        };

        card_shell(
            LED_CYAN,
            title,
            badge,
            pin_btn,
            endpoint,
            actions.into(),
            drawer,
            self.hover.as_deref() == Some(row_id.as_str()),
            row_id,
            industrial,
        )
    }
}

fn opt_str(v: &str) -> Option<String> {
    let t = v.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// An empty/whitespace services_json is valid (no data yet); anything else
/// must parse as a JSON service list.
fn parse_services(json: &str) -> Result<Vec<ServiceConfig>, serde_json::Error> {
    if json.trim().is_empty() {
        Ok(Vec::new())
    } else {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_services_json_is_valid() {
        assert!(parse_services("").unwrap().is_empty());
        assert!(parse_services("  \n ").unwrap().is_empty());
    }

    #[test]
    fn corrupt_services_json_is_err() {
        assert!(parse_services("{not json").is_err());
    }
}

impl AppModel {
    /// True when the trimmed legacy industrial look is selected.
    /// System (default) follows the COSMIC theme via `applet::style()`.
    fn industrial(&self) -> bool {
        self.config.use_industrial()
    }

    /// The active accent scheme, resolved from persisted config.
    /// Only honoured in industrial mode; system mode uses the COSMIC accent.
    fn accent(&self) -> Accent {
        accent_from_name(self.config.accent_name())
    }

    /// Slang overrides for the colloquial nomenclature mode.
    /// Dormant under the reference card layout; the "Pimp my port" toggle
    /// in settings is retained so the mode can return without rework.
    #[allow(dead_code)]
    fn lingo(&self, straight: impl Into<String>, slang: &str) -> String {
        if self.config.colloquial {
            slang.to_string()
        } else {
            straight.into()
        }
    }
}
