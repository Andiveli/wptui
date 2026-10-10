use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SETTINGS_FILE_NAME: &str = "settings.conf";
const COMPOSER_DIRECTION_KEY: &str = "composer_direction";
const MOUSE_KEY: &str = "mouse";

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

pub(crate) fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SETTINGS_FILE_NAME)
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
        // Keep capture opt-in until wheel and click handling are implemented.
        return false;
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
        .unwrap_or(false)
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
    fn mouse_capture_stays_off_until_enabled_in_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = settings_path(directory.path());
        assert!(!load_mouse_capture_enabled(&path));

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
        assert!(!load_mouse_capture_enabled(&path));
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
