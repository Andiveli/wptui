use super::status_list::{StatusList, StatusListItem};
use crate::app::App;
use crate::app::actions::{FocusPane, StatusCompositionState};
use crate::ui::message_list::render_status_messages;
use crate::ui::{action_notice_text, render_composer};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Paragraph, Widget},
};

pub(super) fn render_status_contacts(frame: &mut Frame, app: &mut App, area: Rect) {
    let items = app
        .status_contacts
        .iter()
        .map(|contact| StatusListItem::from_contact(app, contact))
        .collect::<Vec<_>>();

    let block = Block::bordered()
        .title("Status")
        .border_style(
            Style::default().fg(if app.focus_pane == FocusPane::ChatList {
                Color::Green
            } else {
                Color::White
            }),
        );
    let list_area = block.inner(area);
    block.render(area, frame.buffer_mut());

    if items.is_empty() {
        frame.render_widget(Paragraph::new("No statuses yet"), list_area);
        return;
    }
    frame.render_stateful_widget(
        StatusList::new(&items),
        list_area,
        &mut app.status_selection,
    );
}

pub(super) fn render_statuses(frame: &mut Frame, app: &mut App, area: Rect) {
    match app.status_composition {
        StatusCompositionState::Authoring => {
            let [help_area, composer_area] = ratatui::layout::Layout::vertical([
                ratatui::layout::Constraint::Length(1),
                ratatui::layout::Constraint::Min(1),
            ])
            .areas(area);
            let help = action_notice_text(app)
                .unwrap_or_else(|| "Enter publish · Esc cancel · Ctrl+O attach".to_string());
            frame.render_widget(Paragraph::new(help).fg(Color::Cyan), help_area);
            render_composer(frame, app, composer_area, " Status update ", None, None);
            return;
        }
        StatusCompositionState::Submitting => {
            let pending = app.pending_status_sends;
            let feedback = action_notice_text(app)
                .map(|notice| format!("{pending} updates pending\n{notice}"))
                .unwrap_or_else(|| format!("{pending} updates pending"));
            render_composer(
                frame,
                app,
                area,
                " Publishing status ",
                None,
                Some(&feedback),
            );
            return;
        }
        StatusCompositionState::Inactive => {}
    }

    let title = app
        .open_status_contact()
        .map(|contact| app.contact_name(&contact).to_string())
        .unwrap_or_else(|| "Status".to_string());
    let border_color = if app.focus_pane == FocusPane::Conversation {
        Color::Green
    } else {
        Color::White
    };
    let mut block = Block::bordered()
        .title(title)
        .border_style(Style::default().fg(border_color));
    if let Some(notice) = action_notice_text(app) {
        block = block.title(Line::from(notice).right_aligned());
    }
    let content_area = block.inner(area);
    block.render(area, frame.buffer_mut());

    if app.open_status_contact().is_none() {
        frame.render_widget(
            Paragraph::new("Select a contact to view their statuses"),
            content_area,
        );
        return;
    }
    render_status_messages(frame, app, content_area);
}
