use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SETTINGS_FILE_NAME: &str = "settings.conf";
const COMPOSER_DIRECTION_KEY: &str = "composer_direction";

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

pub(crate) fn save_composer_direction(path: &Path, direction: ComposerDirection) -> io::Result<()> {
    fs::write(
        path,
        format!(
            "{COMPOSER_DIRECTION_KEY}={}\n",
            match direction {
                ComposerDirection::Auto => "auto",
                ComposerDirection::Rtl => "rtl",
            }
        ),
    )
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
