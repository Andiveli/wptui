//! Embedded file picker overlay.
//!
//! A pure, table-testable directory browser that follows the same stateful
//! overlay pattern as the existing `share_picker` / `url_picker` / `reaction
//! _picker`: it lives inside the ratatui session, lists the current directory,
//! lets the user descend into folders, go back up to the parent, and confirm
//! one or more files. On confirm the App enqueues each as a pending attachment
//! via `Composer::queue_attachment(path, clipboard::file_kind(path))`.
//!
//! Navigation keys (`h/j/k/l`, arrows, Enter) work out of the box. Typing
//! filters only after `/` (an explicit *search mode*); `Esc` leaves search mode
//! and returns to list navigation so the movement keys are never stolen by the
//! keyboard buffer. `Space` toggles a file into the multi-selection set and
//! `Enter` commits them all.
//!
//! No external process is spawned: the picker stays inside [`crate::ui`]'s
//! terminal session, so it never suspends/resumes raw mode or the alternate
//! screen and never touches the input-reader thread.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

/// A single entry shown in the picker. Directories and files are separated in
/// the flattened listing (directories first, then files, each sorted by name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

impl FileEntry {
    fn marker(&self) -> &str {
        if self.is_dir { "/" } else { "" }
    }
    pub fn display_name(&self) -> String {
        format!("{}{}", self.name, self.marker())
    }
}

/// Directory-listing failure. Callers surface it as an unavailable notice
/// instead of opening a broken picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListDirError {
    NotAFolder,
    ReadFailed,
}

/// Pure, disk-backed directory picker state. All navigation, filtering,
/// multi-selection and confirmation logic is table-testable against a
/// temporary directory tree.
#[derive(Debug)]
pub struct FilePickerState {
    current_dir: PathBuf,
    entries: Vec<FileEntry>,
    pub query: String,
    /// Whether the picker is in search mode (`/`). Only while true do typed
    /// characters feed `query`; otherwise they are left for navigation keys.
    pub searching: bool,
    pub selected: usize,
    pub offset: usize,
    viewport_height: usize,
    /// Files toggled for multi-send with `Space`, keyed by absolute path.
    selected_paths: HashSet<PathBuf>,
}

impl FilePickerState {
    /// Starts a picker rooted at `start`. Fails if the directory cannot be
    /// opened — callers surface that as an unavailable notice instead of
    /// opening a broken picker.
    pub fn open(start: &Path) -> Result<Self, ListDirError> {
        let mut picker = Self {
            current_dir: start.to_path_buf(),
            entries: Vec::new(),
            query: String::new(),
            searching: false,
            selected: 0,
            offset: 0,
            viewport_height: 1,
            selected_paths: HashSet::new(),
        };
        picker.load_current_dir()?;
        Ok(picker)
    }

    pub fn current_dir(&self) -> &Path {
        &self.current_dir
    }

    pub fn set_viewport_height(&mut self, height: usize) {
        self.viewport_height = height.max(1);
        self.keep_selected_visible();
    }

    /// Whether the cursor currently sits on a file (as opposed to a folder).
    pub fn cursor_is_file(&self) -> bool {
        self.visible_entries()
            .get(self.selected)
            .is_some_and(|entry| !entry.is_dir)
    }

    /// Move the selection by `delta` and keep the cursor inside the visible
    /// window, mirroring `SharePicker`.
    pub fn move_selection(&mut self, delta: isize) {
        let len = self.visible_entries().len();
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(len.saturating_sub(1));
        self.keep_selected_visible();
    }

    /// Enter search mode. Typed characters now build the filter query.
    pub fn enter_search(&mut self) {
        self.searching = true;
    }

    /// Leave search mode, returning to plain `h/j/k/l` navigation. Keeps the
    /// current query so the user can keep moving inside the filtered subset.
    pub fn end_search(&mut self) {
        self.searching = false;
    }

    pub fn push_query(&mut self, character: char) {
        self.query.push(character);
        self.reset_search_position();
        self.clamp_selection();
    }

    pub fn backspace_query(&mut self) {
        self.query.pop();
        self.reset_search_position();
        self.clamp_selection();
    }

    /// Entries visible in the current directory, filtered by `query`.
    pub fn visible_entries(&self) -> Vec<&FileEntry> {
        let query = self.query.to_lowercase();
        self.entries
            .iter()
            .filter(|entry| query.is_empty() || entry.name.to_lowercase().contains(&query))
            .collect()
    }

    /// Toggle the file under the cursor in/out of the multi-selection set.
    /// Directories cannot be toggled. Returns whether it is now selected.
    pub fn toggle_selected(&mut self) -> bool {
        let Some(path) = self.cursor_path() else {
            return false;
        };
        if path.is_dir() {
            return self.selected_paths.contains(&path);
        }
        if !self.selected_paths.insert(path.clone()) {
            self.selected_paths.remove(&path);
            false
        } else {
            true
        }
    }

    pub fn selected_count(&self) -> usize {
        self.selected_paths.len()
    }

    pub fn is_selected(&self, path: &Path) -> bool {
        self.selected_paths.contains(path)
    }

    /// The files committed for sending, in a deterministic order: when a
    /// multi-selection exists it wins; otherwise the file under the cursor is
    /// the single selection. Empty when nothing is attachable.
    pub fn pending_paths(&self) -> Vec<PathBuf> {
        if !self.selected_paths.is_empty() {
            let mut sorted: Vec<PathBuf> = self.selected_paths.iter().cloned().collect();
            sorted.sort();
            return sorted;
        }
        match self.visible_entries().get(self.selected) {
            Some(entry) if !entry.is_dir => vec![entry.path.clone()],
            _ => Vec::new(),
        }
    }

    /// Descend into the directory under the cursor. Returns false when the
    /// cursor is not on a directory.
    pub fn descend_current(&mut self) -> bool {
        let Some(path) = self.cursor_path_if_dir() else {
            return false;
        };
        self.enter_directory(&path).is_ok()
    }

    fn cursor_path(&self) -> Option<PathBuf> {
        self.visible_entries()
            .get(self.selected)
            .map(|entry| entry.path.clone())
    }

    fn cursor_path_if_dir(&self) -> Option<PathBuf> {
        self.visible_entries()
            .get(self.selected)
            .filter(|entry| entry.is_dir)
            .map(|entry| entry.path.clone())
    }

    /// Ascend to the parent directory, if any.
    pub fn go_parent(&mut self) -> bool {
        let parent = self.current_dir.parent().map(Path::to_path_buf);
        match parent {
            Some(parent) if parent.as_os_str().is_empty() => false,
            Some(parent) => self.enter_directory(&parent).is_ok(),
            None => false,
        }
    }

    fn enter_directory(&mut self, path: &Path) -> Result<(), ListDirError> {
        if !path.is_dir() {
            return Err(ListDirError::NotAFolder);
        }
        self.current_dir = path.to_path_buf();
        self.load_current_dir()
    }

    fn load_current_dir(&mut self) -> Result<(), ListDirError> {
        self.entries = list_dir(&self.current_dir)?;
        self.query.clear();
        self.reset_search_position();
        Ok(())
    }

    fn reset_search_position(&mut self) {
        self.selected = 0;
        self.offset = 0;
    }

    fn clamp_selection(&mut self) {
        self.selected = self
            .selected
            .min(self.visible_entries().len().saturating_sub(1));
        self.offset = self.offset.min(self.selected);
    }

    fn keep_selected_visible(&mut self) {
        let height = self.viewport_height.max(1);
        if self.selected < self.offset {
            self.offset = self.selected;
        }
        if self.selected >= self.offset.saturating_add(height) {
            self.offset = self.selected + 1 - height;
        }
    }

    pub fn viewport(&self) -> std::ops::Range<usize> {
        let end = self.visible_entries().len();
        let height = self.viewport_height.max(1).min(end);
        let start = self.offset.min(end.saturating_sub(height));
        start..start.saturating_add(height)
    }
}

/// Read and sort the contents of `dir`: directories first, then files, each
/// group ordered by (case-insensitive) name. Hidden entries (names starting
/// with `.`) are skipped, as a plain file dialog does not show them by default.
pub fn list_dir(dir: &Path) -> Result<Vec<FileEntry>, ListDirError> {
    if !dir.is_dir() {
        return Err(ListDirError::NotAFolder);
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).map_err(|_| ListDirError::ReadFailed)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => continue,
        };
        if name.starts_with('.') {
            continue;
        }
        let is_dir = path.is_dir();
        entries.push(FileEntry { name, path, is_dir });
    }
    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a small tree and return its root path with the TempDir still held.
    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("photos")).unwrap();
        fs::create_dir(root.join("docs")).unwrap();
        fs::write(root.join("photos/pic.png"), b"x").unwrap();
        fs::write(root.join("docs/report.pdf"), b"x").unwrap();
        fs::write(root.join("notes.txt"), b"x").unwrap();
        fs::write(root.join("readme.md"), b"x").unwrap();
        fs::write(root.join(".hidden"), b"x").unwrap();
        dir
    }

    fn root_of(dir: &tempfile::TempDir) -> &Path {
        dir.path()
    }

    fn names(picker: &FilePickerState) -> Vec<String> {
        picker
            .visible_entries()
            .iter()
            .map(|entry| entry.name.clone())
            .collect()
    }

    #[test]
    fn open_lists_directories_first_then_files_alphabetically() {
        let dir = tree();
        let picker = FilePickerState::open(root_of(&dir)).unwrap();
        assert_eq!(
            names(&picker),
            vec!["docs", "photos", "notes.txt", "readme.md"]
        );
    }

    #[test]
    fn hidden_entries_are_skipped() {
        let dir = tree();
        let picker = FilePickerState::open(root_of(&dir)).unwrap();
        assert!(!names(&picker).contains(&".hidden".to_string()));
    }

    #[test]
    fn descend_into_folder_and_ascend_back() {
        let dir = tree();
        let mut picker = FilePickerState::open(root_of(&dir)).unwrap();
        assert_eq!(picker.visible_entries()[0].name, "docs");
        assert!(picker.descend_current());
        assert_eq!(names(&picker), vec!["report.pdf"]);
        assert!(picker.go_parent());
        assert_eq!(
            names(&picker),
            vec!["docs", "photos", "notes.txt", "readme.md"]
        );
    }

    #[test]
    fn single_cursor_file_is_the_pending_path() {
        let dir = tree();
        let mut picker = FilePickerState::open(root_of(&dir)).unwrap();
        let last = picker.visible_entries().len() - 1;
        for _ in 0..last {
            picker.move_selection(1);
        }
        assert_eq!(
            picker.pending_paths(),
            vec![root_of(&dir).join("readme.md")]
        );
    }

    #[test]
    fn multi_selection_wins_over_the_cursor_file() {
        let dir = tree();
        let root = root_of(&dir).to_path_buf();
        let mut picker = FilePickerState::open(&root).unwrap();
        // Cursor on 'docs' (0); select notes.txt (2) and readme.md (3).
        picker.move_selection(2);
        assert!(picker.toggle_selected());
        picker.move_selection(1);
        assert!(picker.toggle_selected());
        // Move cursor back to a dir; pending must still be the two files.
        picker.move_selection(-2);
        let expected = vec![root.join("notes.txt"), root.join("readme.md")];
        let mut pending = picker.pending_paths();
        pending.sort();
        assert_eq!(pending, expected);
        assert_eq!(picker.selected_count(), 2);
    }

    #[test]
    fn toggle_unselects_a_file() {
        let dir = tree();
        let root = root_of(&dir).to_path_buf();
        let mut picker = FilePickerState::open(&root).unwrap();
        picker.move_selection(2);
        assert!(picker.toggle_selected());
        assert!(!picker.toggle_selected(), "toggling again must unselect");
        assert_eq!(picker.selected_count(), 0);
    }

    #[test]
    fn search_mode_is_distinct_from_navigation() {
        let dir = tree();
        let mut picker = FilePickerState::open(root_of(&dir)).unwrap();
        assert!(!picker.searching, "nav mode by default");
        picker.enter_search();
        assert!(picker.searching);
        picker.end_search();
        assert!(!picker.searching);
    }

    #[test]
    fn query_filters_listing_and_moves_selection_to_top() {
        let dir = tree();
        let mut picker = FilePickerState::open(root_of(&dir)).unwrap();
        picker.enter_search();
        picker.move_selection(3); // onto 'readme.md'
        picker.push_query('p');
        assert_eq!(names(&picker), vec!["photos"]);
        assert_eq!(picker.selected, 0);
        picker.end_search();
    }

    #[test]
    fn backspace_query_restores_full_listing() {
        let dir = tree();
        let mut picker = FilePickerState::open(root_of(&dir)).unwrap();
        picker.enter_search();
        picker.push_query('n'); // matches notes.txt only
        assert_eq!(picker.visible_entries().len(), 1);
        picker.backspace_query();
        assert_eq!(picker.visible_entries().len(), 4);
    }

    #[test]
    fn selection_clamps_against_count() {
        let dir = tree();
        let mut picker = FilePickerState::open(root_of(&dir)).unwrap();
        picker.move_selection(100);
        assert_eq!(picker.selected, picker.visible_entries().len() - 1);
        picker.move_selection(-100);
        assert_eq!(picker.selected, 0);
    }

    #[test]
    fn open_rejects_a_missing_directory() {
        let dir = tree();
        assert_eq!(
            FilePickerState::open(&root_of(&dir).join("nope")).unwrap_err(),
            ListDirError::NotAFolder
        );
    }
}
