use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SETTINGS_FILE_NAME: &str = "settings.conf";
const COMPOSER_DIRECTION_KEY: &str = "composer_direction";
const MOUSE_KEY: &str = "mouse";
const NOTIFICATIONS_KEY: &str = "notifications";
const NOTIFICATION_SOUND_KEY: &str = "notification_sound";
const THEME_KEY: &str = "theme";

pub(crate) fn load_theme(path: &Path) -> crate::ui::theme::Theme {
    load_theme_choice(path).resolve(path)
}

pub(crate) fn load_custom_theme(path: &Path) -> crate::ui::theme::Theme {
    fs::read_to_string(path.with_file_name("theme.json"))
        .ok()
        .and_then(|json| crate::ui::theme::Theme::from_custom_json(&json))
        .unwrap_or_default()
}

pub(crate) fn load_theme_choice(path: &Path) -> super::theme_selector::ThemeChoice {
    use super::theme_selector::ThemeChoice;

    let Ok(contents) = fs::read_to_string(path) else {
        return ThemeChoice::Default;
    };
    contents
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            if key.trim() != THEME_KEY {
                return None;
            }
            // Preserve the loader's first-recognized-name semantics: default
            // and invalid entries are skipped, but even invalid Custom wins.
            ThemeChoice::ALL
                .into_iter()
                .find(|choice| *choice != ThemeChoice::Default && choice.name() == value.trim())
        })
        .unwrap_or_default()
}

/// Replace all theme entries, leaving every unrelated byte intact. Stage the
/// complete update beside the destination so errors cannot truncate settings.
pub(crate) fn save_theme_choice(
    path: &Path,
    choice: super::theme_selector::ThemeChoice,
) -> io::Result<()> {
    save_setting(path, THEME_KEY, choice.name(), SettingUpdate::AllAtomic)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum NotificationSound {
    #[default]
    Bundled,
    System,
    Custom(PathBuf),
}

pub(crate) fn load_notification_sound(path: &Path) -> NotificationSound {
    let Ok(contents) = fs::read_to_string(path) else {
        return NotificationSound::Bundled;
    };
    contents
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            if key.trim() != NOTIFICATION_SOUND_KEY {
                return None;
            }
            let value = value.trim();
            if value == "system" {
                Some(NotificationSound::System)
            } else if !value.is_empty() && Path::new(value).is_absolute() {
                Some(NotificationSound::Custom(PathBuf::from(value)))
            } else {
                None
            }
        })
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ComposerDirection {
    #[default]
    Auto,
    Rtl,
}

impl ComposerDirection {
    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::Auto => Self::Rtl,
            Self::Rtl => Self::Auto,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Rtl => "RTL",
        }
    }
}

pub(crate) fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join(SETTINGS_FILE_NAME)
}

/// Copy legacy settings once, so subsequent toggles update the config copy without
/// losing the other preferences. Never replace an existing config file.
pub(crate) fn resolve_settings_path(config_dir: &Path, data_dir: &Path) -> io::Result<PathBuf> {
    let config = settings_path(config_dir);
    let legacy = settings_path(data_dir);
    if config == legacy || config.exists() {
        return Ok(config);
    }
    if !legacy.exists() {
        fs::create_dir_all(config_dir)?;
        return Ok(config);
    }
    let contents = fs::read(&legacy)?;
    fs::create_dir_all(config_dir)?;
    publish_settings(&config, |file| {
        use io::Write;
        file.write_all(&contents)
    })?;
    Ok(config)
}

// Stage the complete copy next to the destination, then publish without replacing
// a config file created by another process in the meantime.
fn publish_settings(
    config: &Path,
    write: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let parent = config
        .parent()
        .ok_or_else(|| io::Error::other("settings path has no parent"))?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    write(staged.as_file_mut())?;
    match staged.persist_noclobber(config) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.error),
    }
}

pub(crate) fn load_composer_direction(path: &Path) -> ComposerDirection {
    let Ok(contents) = fs::read_to_string(path) else {
        return ComposerDirection::Auto;
    };

    contents
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            if key.trim() != COMPOSER_DIRECTION_KEY {
                return None;
            }
            match value.trim() {
                "auto" => Some(ComposerDirection::Auto),
                "rtl" => Some(ComposerDirection::Rtl),
                _ => None,
            }
        })
        .unwrap_or_default()
}

pub(crate) fn load_mouse_capture_enabled(path: &Path) -> bool {
    let Ok(contents) = fs::read_to_string(path) else {
        return true;
    };

    contents
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            if key.trim() != MOUSE_KEY {
                return None;
            }
            match value.trim() {
                "enable" => Some(true),
                "disable" => Some(false),
                _ => None,
            }
        })
        .unwrap_or(true)
}

pub(crate) fn load_notifications_enabled(path: &Path) -> bool {
    let Ok(contents) = fs::read_to_string(path) else {
        return true;
    };

    contents
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            if key.trim() != NOTIFICATIONS_KEY {
                return None;
            }
            match value.trim() {
                "enable" => Some(true),
                "disable" => Some(false),
                _ => None,
            }
        })
        .unwrap_or(true)
}

pub(crate) fn save_composer_direction(path: &Path, direction: ComposerDirection) -> io::Result<()> {
    save_setting(
        path,
        COMPOSER_DIRECTION_KEY,
        match direction {
            ComposerDirection::Auto => "auto",
            ComposerDirection::Rtl => "rtl",
        },
        SettingUpdate::First,
    )
}

pub(crate) fn save_notifications_enabled(path: &Path, enabled: bool) -> io::Result<()> {
    save_setting(
        path,
        NOTIFICATIONS_KEY,
        if enabled { "enable" } else { "disable" },
        SettingUpdate::First,
    )
}

enum SettingUpdate {
    First,
    AllAtomic,
}

fn save_setting(
    path: &Path,
    setting_key: &str,
    value: &str,
    update: SettingUpdate,
) -> io::Result<()> {
    let replace_all = matches!(update, SettingUpdate::AllAtomic);
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let setting = format!("{setting_key}={value}");
    let mut updated = String::with_capacity(contents.len() + setting.len() + 1);
    let mut replaced = false;
    for line in contents.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if (!replaced || replace_all)
            && body
                .split_once('=')
                .is_some_and(|(key, _)| key.trim() == setting_key)
        {
            if !replace_all {
                updated.push_str(&setting);
                if line.ends_with('\n') {
                    updated.push('\n');
                }
            }
            replaced = true;
        } else {
            updated.push_str(line);
        }
    }
    if !replaced || replace_all {
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(&setting);
        updated.push('\n');
    }
    match update {
        SettingUpdate::First => fs::write(path, updated),
        SettingUpdate::AllAtomic => replace_settings_atomically(path, &updated),
    }
}

fn replace_settings_atomically(path: &Path, updated: &str) -> io::Result<()> {
    use io::Write;

    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("settings path has no parent"))?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(updated.as_bytes())?;
    staged.as_file().sync_all()?;
    staged.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_loading_is_read_only_and_defaults_without_a_valid_name() {
        let directory = tempfile::tempdir().unwrap();
        let path = settings_path(directory.path());
        assert_eq!(load_theme(&path), crate::ui::theme::Theme::default());
        assert!(!path.exists());
        for value in ["", "invalid", "Kanagawa", "kanagawa-wave"] {
            let contents = format!("theme={value}\nmouse=disable\ncustom=keep\n");
            fs::write(&path, &contents).unwrap();
            assert_eq!(load_theme(&path), crate::ui::theme::Theme::default());
            assert_eq!(fs::read_to_string(&path).unwrap(), contents);
            assert!(!load_mouse_capture_enabled(&path));
        }
        for name in [
            "kanagawa",
            "one-dark",
            "catppuccin",
            "tokyo-night",
            "gruvbox",
        ] {
            let contents = format!(" theme = {name} \ncomposer_direction=rtl\nmouse=disable\n");
            fs::write(&path, &contents).unwrap();
            assert_eq!(load_theme(&path), crate::ui::theme::Theme::from_name(name));
            assert_eq!(load_composer_direction(&path), ComposerDirection::Rtl);
            assert!(!load_mouse_capture_enabled(&path));
            assert_eq!(fs::read_to_string(&path).unwrap(), contents);
        }
        let contents = "theme=invalid\ntheme=one-dark\ntheme=gruvbox\ncustom=keep\n";
        fs::write(&path, contents).unwrap();
        assert_eq!(
            load_theme(&path),
            crate::ui::theme::Theme::from_name("one-dark")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), contents);
    }

    // Inspect resolved roles through existing style accessors, not palette storage.
    fn theme_roles(theme: crate::ui::theme::Theme) -> [ratatui::style::Color; 10] {
        use ratatui::style::{Color, Style};

        let selection = theme.selection(Style::default());
        [
            theme.text(Color::Reset),
            theme.muted(Color::Reset),
            theme.accent(Color::Reset),
            selection.bg.unwrap_or(Color::Reset),
            selection.fg.unwrap_or(Color::Reset),
            theme.unread(Color::Reset),
            theme.attention(Color::Reset),
            theme.border(Color::Reset),
            theme.base().bg.unwrap_or(Color::Reset),
            theme.error(Color::Reset),
        ]
    }

    fn load_custom_fixture(contents: &str) -> crate::ui::theme::Theme {
        let directory = tempfile::tempdir().unwrap();
        let path = settings_path(directory.path());
        let theme_path = directory.path().join("theme.json");
        let settings = "theme=custom\nmouse=disable\ncomposer_direction=rtl\ncustom=keep\n";
        fs::write(&path, settings).unwrap();
        fs::write(&theme_path, contents).unwrap();
        let theme = load_theme(&path);
        assert_eq!(fs::read_to_string(&path).unwrap(), settings);
        assert_eq!(fs::read_to_string(&theme_path).unwrap(), contents);
        assert!(!load_mouse_capture_enabled(&path));
        assert_eq!(load_composer_direction(&path), ComposerDirection::Rtl);
        theme
    }

    #[test]
    fn custom_optional_base_and_colors_inherit_builtin_roles() {
        use crate::ui::theme::Theme;

        for contents in ["{}", r#"{"colors":{}}"#, r#"{"base":"kanagawa"}"#] {
            assert_eq!(load_custom_fixture(contents), Theme::from_name("kanagawa"));
        }
        for base in [
            "kanagawa",
            "one-dark",
            "catppuccin",
            "tokyo-night",
            "gruvbox",
        ] {
            let contents = format!(r#"{{"base":"{base}","colors":{{}}}}"#);
            assert_eq!(load_custom_fixture(&contents), Theme::from_name(base));
        }
    }

    #[test]
    fn custom_accepts_each_known_role_without_changing_inherited_roles() {
        use crate::ui::theme::Theme;
        use ratatui::style::Color;

        for (index, role) in [
            "text",
            "muted",
            "accent",
            "selection",
            "selected_text",
            "unread",
            "attention",
            "border",
            "surface",
            "error",
        ]
        .iter()
        .enumerate()
        {
            let contents = format!(r##"{{"base":"one-dark","colors":{{"{role}":"#aB12Ef"}}}}"##);
            let mut expected = theme_roles(Theme::from_name("one-dark"));
            expected[index] = Color::Rgb(0xab, 0x12, 0xef);
            assert_eq!(
                theme_roles(load_custom_fixture(&contents)),
                expected,
                "{role}"
            );
        }
    }

    #[test]
    fn custom_invalid_documents_fall_back_atomically_without_rewriting_files() {
        use crate::ui::theme::Theme;

        // A valid override accompanies invalid input wherever possible: no
        // prefix of a rejected document may be applied to the legacy theme.
        for contents in [
            r##"{"colors":{"text":"#123456"},"unknown":true}"##,
            r##"{"colors":{"text":"#123456","unknown":"#abcdef"}}"##,
            r##"{"base":"missing","colors":{"text":"#123456"}}"##,
            r##"{"base":"custom","colors":{"text":"#123456"}}"##,
            r##"{"base":"Kanagawa","colors":{"text":"#123456"}}"##,
            r##"{"base":"default","colors":{"text":"#123456"}}"##,
            r##"{"base":"../theme.json","colors":{"text":"#123456"}}"##,
            r##"{"include":"theme.json","colors":{"text":"#123456"}}"##,
            r##"{"path":"theme.json","colors":{"text":"#123456"}}"##,
            r##"{"plugins":[],"colors":{"text":"#123456"}}"##,
            r##"{"code":"ignored","colors":{"text":"#123456"}}"##,
            r##"{"colors":{"text":"#123456"},"base":42}"##,
            r#"{"colors":[]}"#,
            r#"{"colors":null}"#,
            r#"{"base":null}"#,
            r##"{"colors":{"text":"#123456"},"base":"one-dark""##,
            "not json",
            "[]",
            "null",
            "",
        ] {
            assert_eq!(
                load_custom_fixture(contents),
                Theme::default(),
                "{contents}"
            );
        }
    }

    #[test]
    fn custom_colors_require_exact_opaque_rgb_hex_and_reject_the_whole_file() {
        use crate::ui::theme::Theme;

        for value in [
            r##""#123""##,
            r##""#12345""##,
            r##""#1234567""##,
            r##""#123456ff""##,
            r##""123456""##,
            r##""#gg1234""##,
            r##"" #123456""##,
            r##""#123456 ""##,
            r#""red""#,
            r#""transparent""#,
            "123456",
            "null",
            "[]",
        ] {
            let contents = format!(
                r##"{{"base":"gruvbox","colors":{{"text":"#abcdef","surface":{value}}}}}"##
            );
            assert_eq!(load_custom_fixture(&contents), Theme::default(), "{value}");
        }
    }

    #[test]
    fn missing_custom_file_keeps_legacy_without_creating_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = settings_path(directory.path());
        let contents = "theme=custom\nmouse=disable\n";
        fs::write(&path, contents).unwrap();
        assert_eq!(load_theme(&path), crate::ui::theme::Theme::default());
        assert!(!directory.path().join("theme.json").exists());
        assert_eq!(fs::read_to_string(&path).unwrap(), contents);
    }

    #[test]
    fn custom_file_is_ignored_unless_custom_is_selected() {
        use crate::ui::theme::Theme;

        let directory = tempfile::tempdir().unwrap();
        let path = settings_path(directory.path());
        let theme_path = directory.path().join("theme.json");
        for json in [r##"{"colors":{"text":"#123456"}}"##, "not json"] {
            fs::write(&theme_path, json).unwrap();
            for name in [
                "",
                "default",
                "invalid",
                "kanagawa",
                "one-dark",
                "catppuccin",
                "tokyo-night",
                "gruvbox",
            ] {
                let settings = format!("theme={name}\nmouse=disable\n");
                fs::write(&path, &settings).unwrap();
                assert_eq!(load_theme(&path), Theme::from_name(name));
                assert_eq!(fs::read_to_string(&path).unwrap(), settings);
                assert_eq!(fs::read_to_string(&theme_path).unwrap(), json);
            }
        }
    }

    #[test]
    fn custom_lookup_follows_resolved_config_after_migration_and_with_existing_config() {
        use crate::ui::theme::Theme;

        for existing_config in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let config = root.path().join("config");
            let data = root.path().join("data");
            fs::create_dir_all(&config).unwrap();
            fs::create_dir_all(&data).unwrap();
            let legacy = "theme=custom\nmouse=disable\ncustom=legacy\n";
            let active = "theme=custom\nmouse=enable\ncustom=active\n";
            let config_json = r#"{"base":"catppuccin"}"#;
            let legacy_json = r#"{"base":"gruvbox"}"#;
            fs::write(settings_path(&data), legacy).unwrap();
            fs::write(config.join("theme.json"), config_json).unwrap();
            fs::write(data.join("theme.json"), legacy_json).unwrap();
            if existing_config {
                fs::write(settings_path(&config), active).unwrap();
            }
            let path = resolve_settings_path(&config, &data).unwrap();
            assert_eq!(path, settings_path(&config));
            let loaded = load_theme(&path);
            let expected_settings = if existing_config { active } else { legacy };
            assert_eq!(fs::read_to_string(&path).unwrap(), expected_settings);
            assert_eq!(fs::read_to_string(settings_path(&data)).unwrap(), legacy);
            assert_eq!(
                fs::read_to_string(config.join("theme.json")).unwrap(),
                config_json
            );
            assert_eq!(
                fs::read_to_string(data.join("theme.json")).unwrap(),
                legacy_json
            );
            assert_eq!(loaded, Theme::from_name("catppuccin"));
        }
    }

    #[test]
    fn notification_sound_defaults_and_parses_absolute_paths() {
        let directory = tempfile::tempdir().unwrap();
        let path = settings_path(directory.path());
        assert_eq!(load_notification_sound(&path), NotificationSound::Bundled);
        for invalid in [
            "notification_sound=relative.ogg",
            "notification_sound=",
            "notification_sound=systematic",
        ] {
            fs::write(&path, invalid).unwrap();
            assert_eq!(load_notification_sound(&path), NotificationSound::Bundled);
        }
        fs::write(&path, "notification_sound=system\n").unwrap();
        assert_eq!(load_notification_sound(&path), NotificationSound::System);
        fs::write(
            &path,
            "mouse=disable\nnotification_sound=/sounds/alert.mp3\n",
        )
        .unwrap();
        assert_eq!(
            load_notification_sound(&path),
            NotificationSound::Custom(PathBuf::from("/sounds/alert.mp3"))
        );
    }

    #[test]
    fn composer_direction_defaults_to_auto_when_settings_are_absent() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            load_composer_direction(&settings_path(directory.path())),
            ComposerDirection::Auto
        );
    }

    #[test]
    fn composer_direction_persists_and_round_trips() {
        let directory = tempfile::tempdir().unwrap();
        let path = settings_path(directory.path());
        save_composer_direction(&path, ComposerDirection::Rtl).unwrap();
        assert_eq!(load_composer_direction(&path), ComposerDirection::Rtl);
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "composer_direction=rtl\n"
        );
    }

    #[test]
    fn updating_composer_direction_preserves_other_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = settings_path(directory.path());
        fs::write(
            &path,
            "mouse=disable\ncomposer_direction=auto\ncustom=preserve\n",
        )
        .unwrap();

        save_composer_direction(&path, ComposerDirection::Rtl).unwrap();

        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "mouse=disable\ncomposer_direction=rtl\ncustom=preserve\n"
        );
    }

    #[test]
    fn mouse_capture_defaults_on_and_explicit_disable_preserves_native_selection() {
        let directory = tempfile::tempdir().unwrap();
        let path = settings_path(directory.path());
        assert!(load_mouse_capture_enabled(&path));
        let app = crate::app::App::with_data_dir(directory.path(), directory.path());
        assert!(app.mouse_capture_enabled);
        drop(app);

        fs::write(&path, "composer_direction=rtl\nmouse=disable\n").unwrap();
        assert!(!load_mouse_capture_enabled(&path));
        assert_eq!(load_composer_direction(&path), ComposerDirection::Rtl);
        let app = crate::app::App::with_data_dir(directory.path(), directory.path());
        assert!(!app.mouse_capture_enabled);
        drop(app);

        fs::write(&path, "mouse=enable\n").unwrap();
        assert!(load_mouse_capture_enabled(&path));
        let app = crate::app::App::with_data_dir(directory.path(), directory.path());
        assert!(app.mouse_capture_enabled);
        drop(app);

        fs::write(&path, "mouse=invalid\n").unwrap();
        assert!(load_mouse_capture_enabled(&path));
    }

    #[test]
    fn missing_settings_report_unavailable_config_directory() {
        let root = tempfile::tempdir().unwrap();
        let obstacle = root.path().join("not-a-directory");
        fs::write(&obstacle, "blocking file").unwrap();
        let config_dir = obstacle.join("wptui");
        let data_dir = root.path().join("data");
        assert!(resolve_settings_path(&config_dir, &data_dir).is_err());
        assert!(!settings_path(&config_dir).exists());
    }

    #[test]
    fn legacy_settings_move_without_losing_other_preferences() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config");
        let data = root.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let legacy = settings_path(&data);
        fs::write(
            &legacy,
            "mouse=disable\ncomposer_direction=rtl\ncustom=keep\n",
        )
        .unwrap();
        let path = resolve_settings_path(&config, &data).unwrap();
        assert_eq!(path, config.join("settings.conf"));
        assert!(!load_mouse_capture_enabled(&path));
        assert_eq!(load_composer_direction(&path), ComposerDirection::Rtl);
        save_composer_direction(&path, ComposerDirection::Auto).unwrap();
        assert_eq!(
            fs::read_to_string(&legacy).unwrap(),
            "mouse=disable\ncomposer_direction=rtl\ncustom=keep\n"
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "mouse=disable\ncomposer_direction=auto\ncustom=keep\n"
        );
    }

    #[test]
    fn existing_config_takes_precedence_over_legacy() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config");
        let data = root.path().join("data");
        fs::create_dir_all(&config).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(
            settings_path(&config),
            "mouse=enable\ncomposer_direction=auto\n",
        )
        .unwrap();
        fs::write(
            settings_path(&data),
            "mouse=disable\ncomposer_direction=rtl\n",
        )
        .unwrap();
        let path = resolve_settings_path(&config, &data).unwrap();
        assert!(load_mouse_capture_enabled(&path));
        assert_eq!(load_composer_direction(&path), ComposerDirection::Auto);
        assert_eq!(
            fs::read_to_string(settings_path(&data)).unwrap(),
            "mouse=disable\ncomposer_direction=rtl\n"
        );
    }

    #[test]
    fn failed_staging_does_not_publish_partial_settings() {
        use io::Write;

        let root = tempfile::tempdir().unwrap();
        let config = settings_path(root.path());
        let error = publish_settings(&config, |file| {
            file.write_all(b"mouse=disable\n")?;
            Err(io::Error::other("interrupted copy"))
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "interrupted copy");
        assert!(!config.exists());
    }

    #[test]
    fn publication_does_not_replace_existing_config() {
        use io::Write;

        let root = tempfile::tempdir().unwrap();
        let config = settings_path(root.path());
        publish_settings(&config, |file| {
            file.write_all(b"mouse=disable\n")?;
            fs::write(&config, "mouse=enable\n")
        })
        .unwrap();
        assert_eq!(fs::read_to_string(config).unwrap(), "mouse=enable\n");
    }

    #[test]
    fn builtin_theme_settings_survive_migration_and_composer_updates() {
        for theme in [
            "kanagawa",
            "one-dark",
            "catppuccin",
            "tokyo-night",
            "gruvbox",
        ] {
            let root = tempfile::tempdir().unwrap();
            let config = root.path().join("config");
            let data = root.path().join("data");
            fs::create_dir_all(&data).unwrap();
            let original = format!(
                "theme={theme}\nmouse=disable\ncomposer_direction=auto\nnotification_sound=system\ncustom=keep\n"
            );
            let legacy = settings_path(&data);
            fs::write(&legacy, &original).unwrap();

            let path = resolve_settings_path(&config, &data).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
            save_composer_direction(&path, ComposerDirection::Rtl).unwrap();

            let expected = format!(
                "theme={theme}\nmouse=disable\ncomposer_direction=rtl\nnotification_sound=system\ncustom=keep\n"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), expected);
            assert_eq!(fs::read_to_string(&legacy).unwrap(), original);
            assert_eq!(load_composer_direction(&path), ComposerDirection::Rtl);
            assert!(!load_mouse_capture_enabled(&path));
            assert_eq!(load_notification_sound(&path), NotificationSound::System);

            // Resolving on the next startup must keep the updated config copy,
            // not recopy the legacy file and revert unrelated preferences.
            let restored = resolve_settings_path(&config, &data).unwrap();
            assert_eq!(restored, path);
            assert_eq!(fs::read_to_string(restored).unwrap(), expected);
        }
    }

    #[test]
    fn bootstrap_restores_composer_direction_from_the_data_directory() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = crate::app::App::with_data_dir(directory.path(), directory.path());
        app.toggle_composer_direction();
        assert_eq!(app.composer_direction, ComposerDirection::Rtl);
        drop(app);

        let restored = crate::app::App::with_data_dir(directory.path(), directory.path());
        assert_eq!(restored.composer_direction, ComposerDirection::Rtl);
    }
}
