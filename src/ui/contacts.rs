use std::sync::Arc;

use super::contact_list::{
    AVATAR_HEIGHT, AVATAR_WIDTH, CONTACT_ITEM_HEIGHT, ContactList, ContactListItem,
    contact_visible_range,
};
use crate::app::App;
use crate::app::actions::FocusPane;
use crate::app::contact_avatars::prioritized_avatar_requests;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Style},
    widgets::{Block, Paragraph, StatefulWidget, Widget},
};
use ratatui_image::StatefulImage;

pub(super) fn render_contacts(frame: &mut Frame, app: &mut App, area: Rect) {
    let chats = if app.contact_search.input.is_empty() {
        app.sorted_chats.clone()
    } else {
        app.filtered_chats.clone()
    };
    let items = chats
        .iter()
        .map(|chat| ContactListItem::from_chat(app, chat))
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

    let visible = contact_visible_range(
        app.chat_list_state.offset(),
        contacts_area.height,
        chats.len(),
    );
    app.contact_avatars.schedule(
        prioritized_avatar_requests(
            &chats,
            app.chat_list_state.selected(),
            visible.start,
            visible.len(),
        ),
        app.tx.clone(),
        Arc::clone(&app.picker),
    );
    for index in visible {
        let row = index.saturating_sub(app.chat_list_state.offset());
        let y = contacts_area
            .y
            .saturating_add((row * CONTACT_ITEM_HEIGHT) as u16);
        let avatar_area = Rect::new(
            contacts_area.x,
            y,
            AVATAR_WIDTH.min(contacts_area.width),
            AVATAR_HEIGHT.min(contacts_area.bottom().saturating_sub(y)),
        );
        // Partial Kitty placements can leave terminal artifacts after a scroll.
        if avatar_area.width == AVATAR_WIDTH
            && avatar_area.height == AVATAR_HEIGHT
            && let Some(protocol) = app.contact_avatars.protocol_mut(&chats[index])
        {
            StatefulImage::default().render(avatar_area, frame.buffer_mut(), protocol);
        }
    }
}
