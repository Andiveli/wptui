//! Owned selection identity is independent of the resolved palette (Custom may
//! equal a built-in, or fall back to the legacy appearance).
use super::{App, preferences};
use crate::input_key::{Key, KeyCode};
use crate::ui::theme::Theme;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ThemeChoice {
    #[default]
    Default,
    Kanagawa,
    OneDark,
    Catppuccin,
    TokyoNight,
    Gruvbox,
    Custom,
}

impl ThemeChoice {
    pub(crate) const ALL: [Self; 7] = [
        Self::Default,
        Self::Kanagawa,
        Self::OneDark,
        Self::Catppuccin,
        Self::TokyoNight,
        Self::Gruvbox,
        Self::Custom,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Kanagawa => "kanagawa",
            Self::OneDark => "one-dark",
            Self::Catppuccin => "catppuccin",
            Self::TokyoNight => "tokyo-night",
            Self::Gruvbox => "gruvbox",
            Self::Custom => "custom",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Kanagawa => "Kanagawa",
            Self::OneDark => "One Dark",
            Self::Catppuccin => "Catppuccin",
            Self::TokyoNight => "Tokyo Night",
            Self::Gruvbox => "Gruvbox",
            Self::Custom => "Custom",
        }
    }

    pub(crate) fn resolve(self, settings: &std::path::Path) -> Theme {
        if self == Self::Custom {
            preferences::load_custom_theme(settings)
        } else {
            Theme::from_name(self.name())
        }
    }
}

impl App<'_> {
    pub(crate) fn open_theme_selector(&mut self) {
        self.theme_selector = ThemeChoice::ALL
            .iter()
            .position(|choice| *choice == self.theme_choice);
    }

    /// Own every key while open, including unsupported composer/action keys.
    pub(crate) fn handle_theme_selector_key(&mut self, key: &Key) {
        let Some(selected) = self.theme_selector else {
            return;
        };
        if *key == Key::k(KeyCode::Esc) {
            self.theme_selector = None;
        } else if *key == Key::k(KeyCode::Down) || *key == Key::c('j') {
            self.theme_selector = Some((selected + 1).min(ThemeChoice::ALL.len() - 1));
        } else if *key == Key::k(KeyCode::Up) || *key == Key::c('k') {
            self.theme_selector = Some(selected.saturating_sub(1));
        } else if *key == Key::k(KeyCode::Enter) {
            let choice = ThemeChoice::ALL[selected];
            let theme = choice.resolve(&self.preferences_path);
            if let Err(error) = preferences::save_theme_choice(&self.preferences_path, choice) {
                self.unavailable(&format!("Could not save theme: {error}"));
                return;
            }
            self.theme_choice = choice;
            self.theme = theme;
            self.theme_selector = None;
            self.action_notice = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    fn press(app: &mut App<'_>, code: KeyCode) {
        app.on_terminal_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    #[test]
    fn custom_identity_survives_equal_builtin_and_invalid_palettes() {
        for json in ["{}", "not json"] {
            let directory = tempfile::tempdir().unwrap();
            let path = preferences::settings_path(directory.path());
            std::fs::write(&path, "theme=custom\ncustom=keep\n").unwrap();
            std::fs::write(directory.path().join("theme.json"), json).unwrap();
            let mut app = App::with_data_dir(directory.path(), directory.path());
            for character in [' ', 'a', 't'] {
                press(&mut app, KeyCode::Char(character));
            }
            // Enter without navigation must retain Custom, even when its
            // resolved colors equal Kanagawa or the legacy default.
            press(&mut app, KeyCode::Enter);
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                "custom=keep\ntheme=custom\n"
            );
            for character in [' ', 'a', 't'] {
                press(&mut app, KeyCode::Char(character));
            }
            // Reopening still starts at Custom, not the resolved base palette.
            press(&mut app, KeyCode::Up);
            press(&mut app, KeyCode::Enter);
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                "custom=keep\ntheme=gruvbox\n"
            );
            assert_eq!(
                std::fs::read_to_string(directory.path().join("theme.json")).unwrap(),
                json
            );
        }
    }
}
