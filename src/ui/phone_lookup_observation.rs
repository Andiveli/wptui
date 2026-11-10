use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    widgets::{Block, Clear, Paragraph, Wrap},
};

pub(super) fn render(frame: &mut Frame, app: &App) {
    let Some(view) = app.phone_lookup_observation.as_ref() else {
        return;
    };
    let area = super::centered_modal_layout(frame.area());
    if area.is_empty() {
        return;
    }
    let block = Block::bordered().title("Local number response (private)");
    let inner = block.inner(area);
    let [body, hint] = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(inner);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(view.observation.display_text()).scroll((view.row, view.column)),
        body,
    );
    let help = "Esc close | j/k up/down | h/l left/right | Home reset\nLocal only; do not share values";
    frame.render_widget(Paragraph::new(help).wrap(Wrap { trim: false }), hint);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::phone_lookup_observation::ObservationView;
    use crate::app::test_support::TestApp;
    use ratatui::{Terminal, backend::TestBackend};
    use whatsrust::LocalPhoneLookupObservation;

    #[test]
    fn local_view_renders_labels_and_scrolls_at_terminal_sizes() {
        for (width, height) in [(180, 50), (140, 40), (80, 24), (20, 8), (1, 1)] {
            let mut app = TestApp::new();
            let raw = "Requested: \"+15551234567\"\nQuery: \"+15551234567\"\nIsIn: true\nPhoneNumber.User: \"15559876543\"\nJID.User: \"99887766\"";
            let observation = LocalPhoneLookupObservation::from_display_text(raw.into()).unwrap();
            app.phone_lookup_observation = Some(ObservationView::new(observation));
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| render(frame, &app)).unwrap();
            let output = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            if width >= 80 {
                assert!(output.contains("Local number response (private)"));
                assert!(output.contains("Requested: \"+15551234567\""));
                assert!(output.contains("PhoneNumber.User: \"15559876543\""));
                assert!(output.contains("Esc close"));
                app.phone_lookup_observation.as_mut().unwrap().row = 4;
                terminal.draw(|frame| render(frame, &app)).unwrap();
                let output = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>();
                assert!(output.contains("JID.User: \"99887766\""));
                assert!(!output.contains("Requested:"));
            }
        }
    }
}
