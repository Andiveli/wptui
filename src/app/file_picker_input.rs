use crate::app::App;
use crate::app::actions::{AppAction, ConversationMode};
use crate::file_picker::FilePickerState;

impl App<'_> {
    pub(crate) fn dispatch_file_picker_action(&mut self, action: AppAction) {
        match action {
            AppAction::AttachFile => {
                if !self.composer_blocked() {
                    self.open_file_picker();
                }
            }
            AppAction::FilePickerPrevious => self.move_file_picker(-1),
            AppAction::FilePickerNext => self.move_file_picker(1),
            AppAction::FilePickerParent => {
                if !self.file_picker_up() {
                    self.unavailable("Already at the top of the filesystem");
                }
            }
            AppAction::FilePickerDescend => {
                if !self.file_picker_down() {
                    self.unavailable("Cursor is not on a folder");
                }
            }
            AppAction::FilePickerToggle => self.file_picker_toggle(),
            AppAction::FilePickerConfirm => self.confirm_file_picker(),
            AppAction::FilePickerEnterSearch => self.file_picker_enter_search(),
            AppAction::FilePickerEndSearch => self.file_picker_end_search(),
            AppAction::FilePickerBackspace => self.file_picker_backspace(),
            AppAction::FilePickerCharacter(character) => self.file_picker_character(character),
            AppAction::CancelFilePicker => self.cancel_file_picker(),
            _ => unreachable!("non-file-picker action dispatched to file picker"),
        }
    }

    fn open_file_picker(&mut self) {
        if self.file_picker.is_some() {
            return;
        }
        // If a workspace/project root is known, start there; otherwise fall
        // back to the user's home directory or the current directory.
        let start = std::env::var("PROJECT_ROOT")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| home_dir())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        match FilePickerState::open(&start) {
            Ok(picker) => self.file_picker = Some(picker),
            Err(_) => self.unavailable("Could not open the file picker"),
        }
    }

    fn move_file_picker(&mut self, delta: isize) {
        if let Some(picker) = self.file_picker.as_mut() {
            picker.move_selection(delta);
        }
    }

    fn file_picker_up(&mut self) -> bool {
        self.file_picker
            .as_mut()
            .is_some_and(|picker| picker.go_parent())
    }

    fn file_picker_down(&mut self) -> bool {
        self.file_picker
            .as_mut()
            .is_some_and(|picker| picker.descend_current())
    }

    fn file_picker_toggle(&mut self) {
        if let Some(picker) = self.file_picker.as_mut() {
            picker.toggle_selected();
        }
    }

    fn file_picker_enter_search(&mut self) {
        if let Some(picker) = self.file_picker.as_mut() {
            picker.enter_search();
        }
    }

    fn file_picker_end_search(&mut self) {
        if let Some(picker) = self.file_picker.as_mut() {
            picker.end_search();
        }
    }

    fn file_picker_backspace(&mut self) {
        if let Some(picker) = self.file_picker.as_mut() {
            picker.backspace_query();
        }
    }

    fn file_picker_character(&mut self, character: char) {
        if let Some(picker) = self.file_picker.as_mut() {
            picker.push_query(character);
        }
    }

    fn confirm_file_picker(&mut self) {
        if self.composer_blocked() {
            self.file_picker = None;
            return;
        }
        let Some(paths): Option<Vec<std::path::PathBuf>> = self
            .file_picker
            .as_ref()
            .map(FilePickerState::pending_paths)
        else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        self.file_picker = None;
        for path in &paths {
            let kind = crate::clipboard::file_kind(path);
            self.composer
                .queue_attachment(path.to_string_lossy().into_owned().into(), kind);
        }
        // Focus lands straight in the composer so the user can type on top of
        // the just-attached files instead of pressing `i` again.
        self.conversation_mode = ConversationMode::ComposerEditing;
        self.focus_pane = crate::app::actions::FocusPane::Conversation;
    }

    fn cancel_file_picker(&mut self) {
        self.file_picker = None;
        self.action_notice = Some(crate::app::actions::ActionNotice::Cancelled);
    }
}

/// Best-effort home directory lookup for the file picker's default start.
/// Falls back to the current directory when no home is available (e.g. in a
/// headless or minimal environment).
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::TestApp;

    #[test]
    fn parent_without_picker_reports_unavailable_action() {
        let mut app = TestApp::new();

        app.dispatch_file_picker_action(AppAction::FilePickerParent);

        assert!(matches!(
            &app.action_notice,
            Some(crate::app::actions::ActionNotice::Unavailable(message))
                if message.as_str() == "Already at the top of the filesystem"
        ));
    }
}
