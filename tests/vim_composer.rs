use ratatui::crossterm::event::{
    Event, KeyCode as TerminalKeyCode, KeyEvent, KeyModifiers as TerminalKeyModifiers,
};
use wp_tui::app::actions::{
    ActionNotice, ClipboardWriteError, ClipboardWriter, FocusPane, VimComposerMode,
};
mod common;
use common::TestApp;

#[test]
fn vim_navigation_moves_the_caret_without_inserting_commands_or_sending() {
    for (movement, expected) in [
        (vec!['h'], "ab\ncXd"),
        (vec!['h', 'l'], "ab\ncdX"),
        (vec!['k'], "abX\ncd"),
        (vec!['k', 'j'], "ab\ncdX"),
    ] {
        let mut app = TestApp::new();
        app.open_chat_by_jid(whatsrust::JID("alice@s.whatsapp.net".into()));
        app.focus_pane = FocusPane::Conversation;
        app.composer.insert_text("ab\ncd");
        for character in [' ', 'v'] {
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Char(character),
                TerminalKeyModifiers::NONE,
            )));
        }
        for character in movement {
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Char(character),
                TerminalKeyModifiers::NONE,
            )));
        }
        assert_eq!(app.composer.text(), "ab\ncd");
        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Char('i'),
            TerminalKeyModifiers::NONE,
        )));
        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Char('X'),
            TerminalKeyModifiers::NONE,
        )));
        assert_eq!(app.composer.text(), expected);
    }
}

// Each case enters Vim through terminal events and observes the movement through
// the insertion point, rather than inspecting the textarea's private cursor.
fn assert_vim_word_motion(draft: &str, from: usize, keys: &[char], expected: &str) {
    let mut app = TestApp::new();
    app.open_chat_by_jid(whatsrust::JID("alice@s.whatsapp.net".into()));
    app.focus_pane = FocusPane::Conversation;
    app.composer.insert_text(draft);
    let key = |app: &mut TestApp, character| {
        app.on_terminal_event(Event::Key(KeyEvent::new(
            TerminalKeyCode::Char(character),
            TerminalKeyModifiers::NONE,
        )));
    };
    for character in [' ', 'v'] {
        key(&mut app, character);
    }
    // Set up a logical character position using the already supported h motion.
    for _ in from..draft.chars().count() {
        key(&mut app, 'h');
    }
    for &character in keys {
        key(&mut app, character);
    }
    assert_eq!(app.composer.text(), draft, "keys: {keys:?}");
    assert_eq!(app.vim_composer_mode, Some(VimComposerMode::Normal));
    key(&mut app, 'i');
    key(&mut app, 'X');
    assert_eq!(app.composer.text(), expected, "keys: {keys:?}");
}

#[test]
fn vim_w_moves_to_next_small_word_start_across_punctuation_and_lines() {
    assert_vim_word_motion("ab,cd ef", 0, &['w'], "abX,cd ef");
    assert_vim_word_motion("ab,cd ef", 2, &['w'], "ab,Xcd ef");
    assert_vim_word_motion("ab\nβeta", 0, &['w'], "ab\nXβeta");
    assert_vim_word_motion("ab", 2, &['w'], "abX");
}

#[test]
fn vim_b_moves_to_previous_small_word_start() {
    assert_vim_word_motion("ab,cd ef", 6, &['b'], "ab,Xcd ef");
    assert_vim_word_motion("ab,cd", 3, &['b'], "abX,cd");
    assert_vim_word_motion("ab", 0, &['b'], "Xab");
}

#[test]
fn vim_e_moves_to_next_small_word_end() {
    assert_vim_word_motion("ab,cd ef", 0, &['e'], "aXb,cd ef");
    assert_vim_word_motion("ab,cd ef", 1, &['e'], "abX,cd ef");
    assert_vim_word_motion("ab\nβeta", 2, &['e'], "ab\nβetXa");
    assert_vim_word_motion("ab", 1, &['e'], "aXb");
}

#[test]
fn vim_ge_moves_to_previous_small_word_end() {
    assert_vim_word_motion("ab,cd ef", 6, &['g', 'e'], "ab,cXd ef");
    assert_vim_word_motion("ab,cd", 3, &['g', 'e'], "abX,cd");
    assert_vim_word_motion("ab", 0, &['g', 'e'], "Xab");
}

#[test]
fn vim_big_w_skips_punctuation_within_word() {
    assert_vim_word_motion("ab,cd ef", 0, &['W'], "ab,cd Xef");
    assert_vim_word_motion("ab\nβeta", 0, &['W'], "ab\nXβeta");
    assert_vim_word_motion("ab", 2, &['W'], "abX");
}

#[test]
fn vim_big_b_skips_punctuation_within_word() {
    assert_vim_word_motion("ab,cd ef", 6, &['B'], "Xab,cd ef");
    assert_vim_word_motion("ab", 0, &['B'], "Xab");
}

#[test]
fn vim_big_e_skips_punctuation_within_word() {
    assert_vim_word_motion("ab,cd ef", 0, &['E'], "ab,cXd ef");
    assert_vim_word_motion("ab\nβeta", 2, &['E'], "ab\nβetXa");
    assert_vim_word_motion("ab", 1, &['E'], "aXb");
}

#[test]
fn vim_big_ge_skips_punctuation_within_word() {
    assert_vim_word_motion("ab,cd ef", 6, &['g', 'E'], "ab,cXd ef");
    assert_vim_word_motion("ab,cd ef", 0, &['g', 'E'], "Xab,cd ef");
}

#[test]
fn empty_vim_draft_word_motions_do_not_panic_or_modify_text() {
    for motion in [&['b'][..], &['B'], &['g', 'e'], &['g', 'E']] {
        let mut app = TestApp::new();
        app.open_chat_by_jid(whatsrust::JID("empty@s.whatsapp.net".into()));
        app.focus_pane = FocusPane::Conversation;
        for character in [' ', 'v'] {
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Char(character),
                TerminalKeyModifiers::NONE,
            )));
        }
        for &character in motion {
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Char(character),
                TerminalKeyModifiers::NONE,
            )));
        }
        assert_eq!(app.composer.text(), "", "motion: {motion:?}");
        assert_eq!(app.vim_composer_mode, Some(VimComposerMode::Normal));
    }
}

#[test]
fn uppercase_word_motions_accept_shifted_terminal_events() {
    for (start, motion, expected) in [
        (0, vec!['W'], "ab,cd Xef"),
        (6, vec!['B'], "Xab,cd ef"),
        (0, vec!['E'], "ab,cXd ef"),
        (6, vec!['g', 'E'], "ab,cXd ef"),
    ] {
        let mut app = TestApp::new();
        app.open_chat_by_jid(whatsrust::JID("shift@s.whatsapp.net".into()));
        app.focus_pane = FocusPane::Conversation;
        app.composer.insert_text("ab,cd ef");
        for character in [' ', 'v'] {
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Char(character),
                TerminalKeyModifiers::NONE,
            )));
        }
        for _ in start..8 {
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Char('h'),
                TerminalKeyModifiers::NONE,
            )));
        }
        for character in motion {
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Char(character),
                if character.is_ascii_uppercase() {
                    TerminalKeyModifiers::SHIFT
                } else {
                    TerminalKeyModifiers::NONE
                },
            )));
        }
        assert_eq!(app.composer.text(), "ab,cd ef");
        for character in ['i', 'X'] {
            app.on_terminal_event(Event::Key(KeyEvent::new(
                TerminalKeyCode::Char(character),
                TerminalKeyModifiers::NONE,
            )));
        }
        assert_eq!(app.composer.text(), expected);
    }
}

#[derive(Default)]
struct RecordingClipboard(Option<String>);

impl ClipboardWriter for RecordingClipboard {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardWriteError> {
        self.0 = Some(text.into());
        Ok(())
    }

    fn written_text(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

struct FailingClipboard;

impl ClipboardWriter for FailingClipboard {
    fn write_text(&mut self, _: &str) -> Result<(), ClipboardWriteError> {
        Err(ClipboardWriteError)
    }
}

fn vim_yank_key(app: &mut TestApp, character: char) {
    app.on_terminal_event(Event::Key(KeyEvent::new(
        TerminalKeyCode::Char(character),
        if character.is_ascii_uppercase() {
            TerminalKeyModifiers::SHIFT
        } else {
            TerminalKeyModifiers::NONE
        },
    )));
}

fn yank_app(draft: &str, from_end: usize) -> TestApp {
    let mut app = TestApp::new();
    app.open_chat_by_jid(whatsrust::JID("alice@s.whatsapp.net".into()));
    app.focus_pane = FocusPane::Conversation;
    app.composer.insert_text(draft);
    app.clipboard_writer = Box::new(RecordingClipboard::default());
    for character in [' ', 'v'] {
        vim_yank_key(&mut app, character);
    }
    for _ in 0..from_end {
        vim_yank_key(&mut app, 'h');
    }
    app
}

#[test]
fn vim_yank_word_motions_copy_without_moving_the_caret_or_editing() {
    for (draft, from_end, motion, copied, inserted) in [
        ("alpha beta", 10, "yw", "alpha ", "Xalpha beta"),
        ("alpha beta", 10, "ye", "alpha", "Xalpha beta"),
        ("alpha beta", 4, "yb", "alpha ", "alpha Xbeta"),
        ("alpha beta", 4, "yge", "a ", "alpha Xbeta"),
        ("ab,cd ef", 8, "yW", "ab,cd ", "Xab,cd ef"),
    ] {
        let mut app = yank_app(draft, from_end);
        for character in motion.chars() {
            vim_yank_key(&mut app, character);
        }
        assert_eq!(
            app.clipboard_writer.written_text(),
            Some(copied),
            "{motion}"
        );
        assert_eq!(
            app.action_notice,
            Some(ActionNotice::CopiedText(copied.into())),
            "{motion}"
        );
        assert_eq!(app.composer.text(), draft, "{motion}");
        assert_eq!(app.vim_composer_mode, Some(VimComposerMode::Normal));
        vim_yank_key(&mut app, 'i');
        vim_yank_key(&mut app, 'X');
        assert_eq!(app.composer.text(), inserted, "{motion}");
    }
}

#[test]
fn vim_yy_copies_the_current_line_of_a_multiline_draft_without_moving() {
    let mut app = yank_app("first\nsecond\nthird", 12);
    for character in ['y', 'y'] {
        vim_yank_key(&mut app, character);
    }
    assert_eq!(app.clipboard_writer.written_text(), Some("second\n"));
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::CopiedText("second\n".into()))
    );
    assert_eq!(app.composer.text(), "first\nsecond\nthird");
    vim_yank_key(&mut app, 'i');
    vim_yank_key(&mut app, 'X');
    assert_eq!(app.composer.text(), "first\nXsecond\nthird");
}

#[test]
fn pending_vim_y_does_not_copy_and_escape_cancels_it() {
    let mut app = yank_app("alpha beta", 10);
    vim_yank_key(&mut app, 'y');
    assert_eq!(app.clipboard_writer.written_text(), None);
    assert_eq!(app.action_notice, None);
    assert_eq!(app.composer.text(), "alpha beta");
    app.on_terminal_event(Event::Key(KeyEvent::new(
        TerminalKeyCode::Esc,
        TerminalKeyModifiers::NONE,
    )));
    assert_eq!(app.clipboard_writer.written_text(), None);
    assert_eq!(app.action_notice, None);
    assert_eq!(app.composer.text(), "alpha beta");
}

#[test]
fn failed_vim_yank_keeps_draft_and_caret_available_for_editing() {
    let mut app = yank_app("alpha beta", 10);
    app.clipboard_writer = Box::new(FailingClipboard);
    for character in ['y', 'w'] {
        vim_yank_key(&mut app, character);
    }
    assert!(matches!(
        app.action_notice,
        Some(ActionNotice::Unavailable(_))
    ));
    assert_eq!(app.composer.text(), "alpha beta");
    vim_yank_key(&mut app, 'i');
    vim_yank_key(&mut app, 'X');
    assert_eq!(app.composer.text(), "Xalpha beta");
}
