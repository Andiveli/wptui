use std::io::{self, Write};
use std::sync::mpsc::Sender;

use log::error;
use ratatui::crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
};

use crate::app::events::AppInput;
use crate::app::input_reader::InputReader;

// Disabling capture is idempotent. Retry once if the first write was interrupted
// or failed after sending only part of the terminal escape sequence.
fn disable_mouse_with_retry(writer: &mut impl Write) -> io::Result<()> {
    execute!(writer, DisableMouseCapture).or_else(|_| execute!(writer, DisableMouseCapture))
}

/// Owns one terminal-wide mouse capture transition and attempts to release it.
struct MouseCapture<W: Write> {
    writer: W,
    enabled: bool,
}

impl<W: Write> MouseCapture<W> {
    fn new(mut writer: W, enabled: bool) -> io::Result<Self> {
        if enabled {
            if let Err(error) = execute!(writer, EnableMouseCapture) {
                // An interrupted write may already have changed terminal modes.
                if let Err(restore_error) = disable_mouse_with_retry(&mut writer) {
                    return Err(io::Error::new(
                        error.kind(),
                        format!(
                            "mouse capture failed: {error}; mouse restoration uncertain: {restore_error}; run `reset` if your terminal remains in mouse mode"
                        ),
                    ));
                }
                return Err(error);
            }
        }
        Ok(Self { writer, enabled })
    }

    fn stop(&mut self) -> io::Result<()> {
        if self.enabled {
            disable_mouse_with_retry(&mut self.writer)?;
            self.enabled = false;
        }
        Ok(())
    }
}

impl<W: Write> Drop for MouseCapture<W> {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            error!("Failed to release terminal mouse capture: {error}");
        }
    }
}

/// Bracketed paste belongs only to editable chat input, not search or navigation.
struct BracketedPaste<W: Write> {
    writer: W,
    enabled: bool,
}

impl<W: Write> BracketedPaste<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            enabled: false,
        }
    }

    fn sync(&mut self, enabled: bool) -> io::Result<()> {
        if self.enabled == enabled {
            return Ok(());
        }
        if enabled {
            // Even a failed write may have partially enabled the terminal mode.
            self.enabled = true;
            if let Err(error) = execute!(self.writer, EnableBracketedPaste) {
                self.stop().map_err(|restore_error| {
                    io::Error::new(
                        error.kind(),
                        format!("{error}; paste restoration failed: {restore_error}"),
                    )
                })?;
                return Err(error);
            }
            Ok(())
        } else {
            self.stop()
        }
    }

    fn stop(&mut self) -> io::Result<()> {
        if self.enabled {
            execute!(self.writer, DisableBracketedPaste)
                .or_else(|_| execute!(self.writer, DisableBracketedPaste))?;
            self.enabled = false;
        }
        Ok(())
    }
}

impl<W: Write> Drop for BracketedPaste<W> {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            error!("Failed to restore terminal paste mode: {error}");
        }
    }
}

/// Owns terminal setup and restoration for the application UI.
pub(crate) struct TerminalSession {
    terminal: Option<ratatui::DefaultTerminal>,
    mouse_capture: Option<MouseCapture<io::Stdout>>,
    bracketed_paste: BracketedPaste<io::Stdout>,
}

impl TerminalSession {
    pub(crate) fn try_new(mouse_capture_enabled: bool) -> io::Result<Self> {
        let terminal = ratatui::try_init()?;
        let mouse_capture =
            MouseCapture::new(io::stdout(), mouse_capture_enabled).map_err(|error| {
                ratatui::restore();
                error
            })?;
        Ok(Self {
            terminal: Some(terminal),
            mouse_capture: Some(mouse_capture),
            bracketed_paste: BracketedPaste::new(io::stdout()),
        })
    }

    pub(crate) fn sync_paste_mode(&mut self, enabled: bool) -> io::Result<()> {
        self.bracketed_paste.sync(enabled)
    }

    pub(crate) fn terminal_mut(&mut self) -> &mut ratatui::DefaultTerminal {
        self.terminal
            .as_mut()
            .expect("terminal session must be active")
    }

    pub(crate) fn start_input_reader(&self, input_reader: &mut InputReader, tx: Sender<AppInput>) {
        input_reader.start(tx);
    }

    pub(crate) fn stop_input_reader(&self, input_reader: &mut InputReader) {
        input_reader.stop();
    }

    pub(crate) fn restore(mut self) {
        self.restore_terminal();
    }

    fn restore_terminal(&mut self) {
        if let Err(error) = self.bracketed_paste.stop() {
            // Session Drop retries if explicit restoration could not complete.
            error!("Failed to restore terminal paste mode: {error}");
        }
        let mut capture_uncertain = false;
        if let Some(mut capture) = self.mouse_capture.take() {
            if let Err(error) = capture.stop() {
                // Drop makes one further bounded attempt before Ratatui restores.
                error!("Failed to release terminal mouse capture: {error}");
                capture_uncertain = true;
            }
        }
        if self.terminal.take().is_some() {
            ratatui::restore();
        }
        if capture_uncertain {
            eprintln!(
                "Mouse mode restoration is uncertain; run `reset` if your terminal remains in mouse mode."
            );
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.restore_terminal();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct RecordingWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for RecordingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct FailFirstWriter {
        output: Arc<Mutex<Vec<u8>>>,
        first_write: bool,
    }

    impl Write for FailFirstWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.first_write {
                self.first_write = false;
                return Err(io::Error::other("capture failed"));
            }
            self.output.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct InterruptedCaptureWriter {
        output: Arc<Mutex<Vec<u8>>>,
        stage: u8,
    }

    impl Write for InterruptedCaptureWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.stage == 0 && bytes.starts_with(b"\x1b[?1000h") {
                self.stage = 1;
                self.output.lock().unwrap().extend_from_slice(&bytes[..3]);
                return Ok(3);
            }
            if self.stage == 1 {
                self.stage = 2;
                return Err(io::Error::other("partial enable"));
            }
            if self.stage == 2 && bytes.starts_with(b"\x1b[?1000l") {
                self.stage = 3;
                return Err(io::Error::other("first disable failed"));
            }
            self.output.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn paste_mode_switches_only_on_transitions_and_restores_on_drop() {
        let output = Arc::new(Mutex::new(Vec::new()));
        {
            let mut paste = BracketedPaste::new(RecordingWriter(Arc::clone(&output)));
            paste.sync(false).unwrap();
            assert!(output.lock().unwrap().is_empty());
            paste.sync(true).unwrap();
            paste.sync(true).unwrap();
            paste.sync(false).unwrap();
            paste.sync(false).unwrap();
            assert_eq!(*output.lock().unwrap(), b"\x1b[?2004h\x1b[?2004l");
            paste.sync(true).unwrap();
        }
        assert_eq!(
            *output.lock().unwrap(),
            b"\x1b[?2004h\x1b[?2004l\x1b[?2004h\x1b[?2004l"
        );
    }

    #[test]
    fn composer_eligibility_switches_paste_mode_off_outside_editing_and_during_logout() {
        use crate::app::actions::{ConversationMode, FocusPane, Section, VimComposerMode};
        use crate::app::test_support::TestApp;

        let output = Arc::new(Mutex::new(Vec::new()));
        let mut paste = BracketedPaste::new(RecordingWriter(Arc::clone(&output)));
        let mut app = TestApp::new();
        paste.sync(app.accepts_terminal_paste()).unwrap();
        assert!(output.lock().unwrap().is_empty());
        app.focus_pane = FocusPane::Conversation;
        app.selected_section = Section::Communities;
        app.conversation_mode = ConversationMode::ComposerEditing;
        paste.sync(app.accepts_terminal_paste()).unwrap();
        app.vim_composer_mode = Some(VimComposerMode::Normal);
        paste.sync(app.accepts_terminal_paste()).unwrap();
        app.vim_composer_mode = Some(VimComposerMode::Insert);
        paste.sync(app.accepts_terminal_paste()).unwrap();
        app.pending_logout = true;
        paste.sync(app.accepts_terminal_paste()).unwrap();
        app.pending_logout = false;
        app.conversation_mode = ConversationMode::EditingMessage;
        paste.sync(app.accepts_terminal_paste()).unwrap();
        app.focus_pane = FocusPane::ChatList;
        paste.sync(app.accepts_terminal_paste()).unwrap();
        drop(paste);
        assert_eq!(
            *output.lock().unwrap(),
            b"\x1b[?2004h\x1b[?2004l\x1b[?2004h\x1b[?2004l\x1b[?2004h\x1b[?2004l"
        );
    }

    #[test]
    fn paste_explicit_stop_is_idempotent_and_failed_enable_restores() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let mut paste = BracketedPaste::new(FailFirstWriter {
            output: Arc::clone(&output),
            first_write: true,
        });
        assert!(paste.sync(true).is_err());
        assert_eq!(*output.lock().unwrap(), b"\x1b[?2004l");
        paste.sync(true).unwrap();
        paste.stop().unwrap();
        let released = output.lock().unwrap().clone();
        paste.stop().unwrap();
        drop(paste);
        assert_eq!(*output.lock().unwrap(), released);
    }

    #[test]
    fn partial_enable_and_failed_first_disable_retry_restoration() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let writer = InterruptedCaptureWriter {
            output: Arc::clone(&output),
            stage: 0,
        };
        assert!(MouseCapture::new(writer, true).is_err());
        assert!(
            output
                .lock()
                .unwrap()
                .windows(8)
                .any(|part| part == b"\x1b[?1000l")
        );
    }

    struct FailFirstDisableWriter {
        output: Arc<Mutex<Vec<u8>>>,
        failed: bool,
    }

    impl Write for FailFirstDisableWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.failed && bytes.starts_with(b"\x1b[?1000l") {
                self.failed = true;
                return Err(io::Error::other("first disable failed"));
            }
            self.output.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn failed_first_shutdown_disable_retries_before_restoration() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let writer = FailFirstDisableWriter {
            output: Arc::clone(&output),
            failed: false,
        };
        let mut capture = MouseCapture::new(writer, true).unwrap();
        capture.stop().unwrap();
        let released = output.lock().unwrap().clone();
        assert!(released.windows(8).any(|part| part == b"\x1b[?1000l"));
        drop(capture);
        assert_eq!(*output.lock().unwrap(), released);
    }

    #[test]
    fn failed_enable_attempts_to_restore_mouse_mode() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let writer = FailFirstWriter {
            output: Arc::clone(&output),
            first_write: true,
        };
        assert!(MouseCapture::new(writer, true).is_err());
        assert!(
            output
                .lock()
                .unwrap()
                .windows(8)
                .any(|part| part == b"\x1b[?1000l")
        );
    }

    struct FailedRecoveryWriter {
        failed_enable: bool,
    }

    impl Write for FailedRecoveryWriter {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            if !self.failed_enable {
                self.failed_enable = true;
                return Err(io::Error::other("enable failed"));
            }
            Err(io::Error::other("terminal output unavailable"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn unrecoverable_enable_error_reports_manual_terminal_reset() {
        let error = MouseCapture::new(
            FailedRecoveryWriter {
                failed_enable: false,
            },
            true,
        )
        .err()
        .expect("capture should fail when terminal output fails");
        assert!(error.to_string().contains("run `reset`"));
    }

    #[test]
    fn capture_enables_and_drop_restores_mouse_mode() {
        let output = Arc::new(Mutex::new(Vec::new()));
        {
            let _capture = MouseCapture::new(RecordingWriter(Arc::clone(&output)), true).unwrap();
            assert!(
                output
                    .lock()
                    .unwrap()
                    .windows(8)
                    .any(|part| part == b"\x1b[?1000h")
            );
        }
        assert!(
            output
                .lock()
                .unwrap()
                .windows(8)
                .any(|part| part == b"\x1b[?1000l")
        );
    }

    #[test]
    fn capture_disabled_does_not_change_terminal_mode() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let mut capture = MouseCapture::new(RecordingWriter(Arc::clone(&output)), false).unwrap();
        capture.stop().unwrap();
        drop(capture);
        assert!(output.lock().unwrap().is_empty());
    }

    #[test]
    fn explicit_stop_releases_mouse_once() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let mut capture = MouseCapture::new(RecordingWriter(Arc::clone(&output)), true).unwrap();
        capture.stop().unwrap();
        let released = output.lock().unwrap().clone();
        capture.stop().unwrap();
        drop(capture);
        assert_eq!(*output.lock().unwrap(), released);
    }
}
