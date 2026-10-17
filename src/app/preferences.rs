use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SETTINGS_FILE_NAME: &str = "settings.conf";
const COMPOSER_DIRECTION_KEY: &str = "composer_direction";
const MOUSE_KEY: &str = "mouse";
const NOTIFICATION_SOUND_KEY: &str = "notification_sound";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum NotificationSound {
    #[default]
    System,
    Custom(PathBuf),
}

pub(crate) fn load_notification_sound(path: &Path) -> NotificationSound {
    let Ok(contents) = fs::read_to_string(path) else {
        return NotificationSound::System;
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

pub(crate) fn save_composer_direction(path: &Path, direction: ComposerDirection) -> io::Result<()> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let setting = format!(
        "{COMPOSER_DIRECTION_KEY}={}",
        match direction {
            ComposerDirection::Auto => "auto",
            ComposerDirection::Rtl => "rtl",
        }
    );
    let mut updated = String::with_capacity(contents.len() + setting.len() + 1);
    let mut replaced = false;
    for line in contents.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if !replaced
            && body
                .split_once('=')
                .is_some_and(|(key, _)| key.trim() == COMPOSER_DIRECTION_KEY)
        {
            updated.push_str(&setting);
            if line.ends_with('\n') {
                updated.push('\n');
            }
            replaced = true;
        } else {
            updated.push_str(line);
        }
    }
    if !replaced {
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(&setting);
        updated.push('\n');
    }
    fs::write(path, updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_sound_defaults_and_parses_absolute_paths() {
        let directory = tempfile::tempdir().unwrap();
        let path = settings_path(directory.path());
        assert_eq!(load_notification_sound(&path), NotificationSound::System);
        for invalid in [
            "notification_sound=relative.ogg",
            "notification_sound=",
            "notification_sound=systematic",
        ] {
            fs::write(&path, invalid).unwrap();
            assert_eq!(load_notification_sound(&path), NotificationSound::System);
        }
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
