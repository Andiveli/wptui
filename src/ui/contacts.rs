use std::sync::Arc;

use super::contact_list::{
    AVATAR_HEIGHT, AVATAR_WIDTH, ContactList, ContactListItem, contact_visible_range,
    visible_contact_rows,
};
use crate::app::App;
use crate::app::actions::FocusPane;
use crate::app::contact_avatars::AvatarTarget;
use crate::app::contact_avatars::prioritized_avatar_requests;
use crate::app::runtime_diagnostics::Phase;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Style},
    widgets::{Block, Paragraph, StatefulWidget, Widget},
};
use ratatui_image::StatefulImage;

pub(crate) fn render_contacts(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = app.visible_contact_rows();
    let targets = rows
        .iter()
        .filter_map(|row| row.avatar_target().cloned().map(AvatarTarget::Contact))
        .collect::<Vec<_>>();
    let items = rows
        .iter()
        .map(|row| ContactListItem::from_contact_row(app, row))
        .collect::<Vec<_>>();

    let mut list_area = area;
    if !app.contact_search.input.is_empty() || app.contact_search_active {
        let [search_area, new_list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Percentage(100)]).areas(area);
        list_area = new_list_area;

        let text = format!("/{}", app.contact_search.input);
        frame.render_widget(Paragraph::new(text), search_area);

        if app.contact_search_active {
            frame.set_cursor_position(Position::new(
                search_area.x + app.contact_search.character_index as u16 + 1,
                search_area.y,
            ));
        }
    }

    let block = Block::bordered()
        .title(if let Some(p) = app.history_sync_percent {
            format!("Contacts ({p}%)")
        } else {
            "Contacts".to_string()
        })
        .border_style(
            Style::default().fg(if app.focus_pane == FocusPane::ChatList {
                Color::Green
            } else {
                Color::White
            }),
        );
    let contacts_area = block.inner(list_area);
    block.render(list_area, frame.buffer_mut());
    frame.render_stateful_widget(
        ContactList::new(&items),
        contacts_area,
        &mut app.chat_list_state,
    );

    let visible = visible_contact_rows(&items, app.chat_list_state.offset(), contacts_area.height);
    let _legacy_visible_range = contact_visible_range(
        app.chat_list_state.offset(),
        contacts_area.height,
        rows.len(),
    );
    let avatar_started = app.runtime_diagnostics.phase_started();
    app.contact_avatars.schedule(
        prioritized_avatar_requests(&targets, None, 0, targets.len()),
        app.tx.clone(),
        Arc::clone(&app.picker),
    );
    if let Some(started) = avatar_started {
        app.runtime_diagnostics
            .record_phase_finished(Phase::AvatarScheduling, started);
    }
    for (index, relative_y) in visible {
        let y = contacts_area.y.saturating_add(relative_y);
        let avatar_area = Rect::new(
            contacts_area.x,
            y,
            AVATAR_WIDTH.min(contacts_area.width),
            AVATAR_HEIGHT.min(contacts_area.bottom().saturating_sub(y)),
        );
        // Partial Kitty placements can leave terminal artifacts after a scroll.
        if avatar_area.width == AVATAR_WIDTH
            && avatar_area.height == AVATAR_HEIGHT
            && let Some(target) = rows[index]
                .avatar_target()
                .map(|jid| AvatarTarget::Contact(jid.clone()))
            && let Some(protocol) = app.contact_avatars.protocol_mut(&target)
        {
            StatefulImage::default().render(avatar_area, frame.buffer_mut(), protocol);
        }
    }
}
