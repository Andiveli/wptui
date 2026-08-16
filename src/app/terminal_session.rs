use std::io;
use std::sync::mpsc::Sender;

use crate::app::events::AppInput;
use crate::app::input_reader::InputReader;

/// Owns terminal setup and restoration for the application UI.
pub(crate) struct TerminalSession {
    terminal: Option<ratatui::DefaultTerminal>,
}

impl TerminalSession {
    pub(crate) fn try_new() -> io::Result<Self> {
        ratatui::try_init().map(|terminal| Self {
            terminal: Some(terminal),
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
