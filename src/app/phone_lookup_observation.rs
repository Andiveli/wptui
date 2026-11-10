use super::App;
use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind, MouseEventKind};
use whatsrust::LocalPhoneLookupObservation;

#[derive(Debug)]
pub(crate) struct ObservationView {
    pub(crate) observation: LocalPhoneLookupObservation,
    pub(crate) row: u16,
    pub(crate) column: u16,
    last_row: u16,
    last_column: u16,
}

impl ObservationView {
    pub(crate) fn new(observation: LocalPhoneLookupObservation) -> Self {
        let text = observation.display_text();
        let last_row = text.lines().count().saturating_sub(1) as u16;
        let last_column = text
            .lines()
            .map(str::len)
            .max()
            .unwrap_or(0)
            .saturating_sub(1) as u16;
        Self {
            observation,
            row: 0,
            column: 0,
            last_row,
            last_column,
        }
    }

    fn move_rows(&mut self, delta: i16) {
        self.row = self.row.saturating_add_signed(delta).min(self.last_row);
    }

    fn move_columns(&mut self, delta: i16) {
        self.column = self
            .column
            .saturating_add_signed(delta)
            .min(self.last_column);
    }
}

impl App<'_> {
    pub(crate) fn handle_phone_lookup_observation_input(&mut self, event: &Event) -> bool {
        let Some(view) = self.phone_lookup_observation.as_mut() else {
            return false;
        };
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Esc => self.phone_lookup_observation = None,
                KeyCode::Down | KeyCode::Char('j') => view.move_rows(1),
                KeyCode::Up | KeyCode::Char('k') => view.move_rows(-1),
                KeyCode::Right | KeyCode::Char('l') => view.move_columns(1),
                KeyCode::Left | KeyCode::Char('h') => view.move_columns(-1),
                KeyCode::PageDown => view.move_rows(10),
                KeyCode::PageUp => view.move_rows(-10),
                KeyCode::Home => {
                    view.row = 0;
                    view.column = 0;
                }
                _ => {}
            },
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => view.move_rows(1),
                MouseEventKind::ScrollUp => view.move_rows(-1),
                _ => {}
            },
            _ => {}
        }
        // Every event is modal: Enter, text, clipboard and mouse clicks cannot
        // reach the composer or open/select a different chat underneath.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::TestApp;
    use ratatui::crossterm::event::{KeyEvent, KeyModifiers};

    #[test]
    fn local_observation_captures_input_scrolls_and_drops_on_escape() {
        let mut app = TestApp::new();
        let raw = "Requested: \"+15551234567\"\nJID.User: \"99887766\"";
        let observation = LocalPhoneLookupObservation::from_display_text(raw.into()).unwrap();
        app.phone_lookup_observation = Some(ObservationView::new(observation));
        assert!(!format!("{:?}", app.phone_lookup_observation).contains("15551234567"));
        let search_before = app.contact_search.input.to_string();
        for code in [
            KeyCode::Char('x'),
            KeyCode::Enter,
            KeyCode::Down,
            KeyCode::Right,
        ] {
            app.on_terminal_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
        }
        let view = app.phone_lookup_observation.as_ref().unwrap();
        assert_eq!((view.row, view.column), (1, 1));
        assert!(app.chats.is_empty());
        assert!(app.chat_messages.is_empty());
        assert_eq!(app.contact_search.input.to_string(), search_before);
        app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(app.phone_lookup_observation.is_none());
        assert!(app.chats.is_empty());
    }
}
