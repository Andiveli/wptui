//! Built-in dark UI palettes and validated user overrides. Built-in UI/base
//! roles are adapted from Shiki's bundled themes; see THIRD_PARTY_NOTICES.md.
use ratatui::style::{Color, Style};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Palette {
    text: Color,
    muted: Color,
    accent: Color,
    selection: Color,
    selected_text: Color,
    unread: Color,
    attention: Color,
    border: Color,
    surface: Color,
    error: Color,
}

/// Resolved at bootstrap or when applying a selection; None retains legacy styles.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Theme(Option<Palette>);

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

impl Theme {
    pub fn from_name(name: &str) -> Self {
        let palette = match name {
            "kanagawa" => Palette {
                text: rgb(0xdcd7ba),
                muted: rgb(0xc8c093),
                accent: rgb(0x7e9cd8),
                selection: rgb(0x363646),
                selected_text: rgb(0xdcd7ba),
                unread: rgb(0x98bb6c),
                attention: rgb(0xe6c384),
                border: rgb(0xc8c093),
                surface: rgb(0x1f1f28),
                error: rgb(0xff5d62),
            },
            "one-dark" => Palette {
                text: rgb(0xabb2bf),
                muted: rgb(0x9da5b4),
                accent: rgb(0x4aa5f0),
                selection: rgb(0x2c313a),
                selected_text: rgb(0xd7dae0),
                unread: rgb(0xa5e075),
                attention: rgb(0xf0a45d),
                border: rgb(0x9da5b4),
                surface: rgb(0x282c34),
                error: rgb(0xff616e),
            },
            "catppuccin" => Palette {
                text: rgb(0xcdd6f4),
                muted: rgb(0xa6adc8),
                accent: rgb(0x89b4fa),
                selection: rgb(0x313244),
                selected_text: rgb(0xcdd6f4),
                unread: rgb(0xa6e3a1),
                attention: rgb(0xf9e2af),
                border: rgb(0xa6adc8),
                surface: rgb(0x1e1e2e),
                error: rgb(0xf38ba8),
            },
            "tokyo-night" => Palette {
                text: rgb(0xa9b1d6),
                muted: rgb(0xacb0d0),
                accent: rgb(0x7aa2f7),
                selection: rgb(0x202330),
                selected_text: rgb(0xa9b1d6),
                unread: rgb(0x73daca),
                attention: rgb(0xe0af68),
                border: rgb(0xacb0d0),
                surface: rgb(0x1a1b26),
                error: rgb(0xf7768e),
            },
            "gruvbox" => Palette {
                text: rgb(0xebdbb2),
                muted: rgb(0xa89984),
                accent: rgb(0x83a598),
                selection: rgb(0x3c3836),
                selected_text: rgb(0xebdbb2),
                unread: rgb(0xb8bb26),
                attention: rgb(0xfabd2f),
                border: rgb(0xa89984),
                surface: rgb(0x282828),
                error: rgb(0xfb4934),
            },
            _ => return Self::default(),
        };
        Self(Some(palette))
    }

    /// Resolve the entire document before publishing any overrides. Invalid
    /// input returns None so the caller can retain the legacy default theme.
    pub(crate) fn from_custom_json(contents: &str) -> Option<Self> {
        let document: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(contents).ok()?;
        if document.keys().any(|key| key != "base" && key != "colors") {
            return None;
        }
        let base = match document.get("base") {
            Some(value) => value.as_str()?,
            None => "kanagawa",
        };
        let mut palette = Self::from_name(base).0?;
        if let Some(colors) = document.get("colors") {
            for (role, value) in colors.as_object()? {
                let color = value.as_str()?;
                if color.len() != 7
                    || !color.starts_with('#')
                    || !color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
                {
                    return None;
                }
                let color = rgb(u32::from_str_radix(&color[1..], 16).ok()?);
                let target = match role.as_str() {
                    "text" => &mut palette.text,
                    "muted" => &mut palette.muted,
                    "accent" => &mut palette.accent,
                    "selection" => &mut palette.selection,
                    "selected_text" => &mut palette.selected_text,
                    "unread" => &mut palette.unread,
                    "attention" => &mut palette.attention,
                    "border" => &mut palette.border,
                    "surface" => &mut palette.surface,
                    "error" => &mut palette.error,
                    _ => return None,
                };
                *target = color;
            }
        }
        Some(Self(Some(palette)))
    }

    pub(crate) fn base(self) -> Style {
        self.0.map_or(Style::default(), |p| {
            Style::default().fg(p.text).bg(p.surface)
        })
    }

    pub(crate) fn overlay(self) -> Style {
        if self.0.is_some() {
            self.base()
        } else {
            Style::default().bg(Color::Black)
        }
    }

    pub(crate) fn secondary(self, legacy: Style) -> Style {
        self.0.map_or(legacy, |p| legacy.fg(p.muted).bg(p.surface))
    }

    pub(crate) fn selection(self, legacy: Style) -> Style {
        self.0.map_or(legacy, |p| {
            Style::default().fg(p.selected_text).bg(p.selection)
        })
    }

    pub(crate) fn text(self, legacy: Color) -> Color {
        self.0.map_or(legacy, |p| p.text)
    }

    pub(crate) fn muted(self, legacy: Color) -> Color {
        self.0.map_or(legacy, |p| p.muted)
    }

    pub(crate) fn accent(self, legacy: Color) -> Color {
        self.0.map_or(legacy, |p| p.accent)
    }

    pub(crate) fn unread_style(self, legacy: Style) -> Style {
        self.0.map_or(legacy, |p| legacy.fg(p.unread).bg(p.surface))
    }

    pub(crate) fn unread(self, legacy: Color) -> Color {
        self.0.map_or(legacy, |p| p.unread)
    }

    pub(crate) fn attention(self, legacy: Color) -> Color {
        self.0.map_or(legacy, |p| p.attention)
    }

    pub(crate) fn border(self, legacy: Color) -> Color {
        self.0.map_or(legacy, |p| p.border)
    }

    pub(crate) fn error(self, legacy: Color) -> Color {
        self.0.map_or(legacy, |p| p.error)
    }
}
