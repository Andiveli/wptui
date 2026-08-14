use super::status_list::{StatusList, StatusListItem};
use crate::app::App;
use crate::app::actions::FocusPane;
use crate::ui::message_list::render_status_messages;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
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
    let title = app
        .open_status_contact()
        .map(|contact| app.contact_name(&contact).to_string())
        .unwrap_or_else(|| "Status".to_string());
    let border_color = if app.focus_pane == FocusPane::Conversation {
        Color::Green
    } else {
        Color::White
    };
    let block = Block::bordered()
        .title(title)
        .border_style(Style::default().fg(border_color));
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
