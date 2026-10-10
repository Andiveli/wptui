use std::io::{self, Write};
use std::sync::mpsc::Sender;

use log::error;
use ratatui::crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
};

use crate::app::events::AppInput;
use crate::app::input_reader::InputReader;

/// Owns one terminal-wide mouse capture transition and always releases it.
struct MouseCapture<W: Write> {
    writer: W,
    enabled: bool,
}

impl<W: Write> MouseCapture<W> {
    fn new(mut writer: W, enabled: bool) -> io::Result<Self> {
        if enabled {
            if let Err(error) = execute!(writer, EnableMouseCapture) {
                // An interrupted write may already have changed terminal modes.
                let _ = execute!(writer, DisableMouseCapture);
                return Err(error);
            }
        }
        Ok(Self { writer, enabled })
    }

    fn stop(&mut self) -> io::Result<()> {
        if self.enabled {
            execute!(self.writer, DisableMouseCapture)?;
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

/// Owns terminal setup and restoration for the application UI.
pub(crate) struct TerminalSession {
    terminal: Option<ratatui::DefaultTerminal>,
    mouse_capture: Option<MouseCapture<io::Stdout>>,
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
        })
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
        if let Some(mut capture) = self.mouse_capture.take() {
            if let Err(error) = capture.stop() {
                error!("Failed to release terminal mouse capture: {error}");
            }
        }
        if self.terminal.take().is_some() {
            ratatui::restore();
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
