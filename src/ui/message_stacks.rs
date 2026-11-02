//! Presentation-only stacks keep every original ID and navigation index.
use chrono::{DateTime, Local};
use whatsrust as wr;

use crate::app::App;

use super::AuthorGroupContext;

pub(super) fn project(
    app: &mut App,
    items: &[wr::MessageId],
    groups: &[AuthorGroupContext],
    unread_count: usize,
) -> Vec<AuthorGroupContext> {
    let mut projected = groups.to_vec();
    let selected = app
        .message_list_state
        .selected
        .or_else(|| {
            let id = app.message_list_state.get_selected_message()?;
            items.iter().position(|item| item == &id)
        })
        .map(|index| index.min(items.len().saturating_sub(1)));
    let mut fronts = Vec::new();
    let mut start = 0;
    while start < items.len() {
        let mut end = start + 1;
        while end < items.len()
            && end != unread_count
            && can_join(app, &items[end - 1], &items[end])
        {
            end += 1;
        }
        if end - start > 1 {
            let front = selected
                .filter(|index| (start..end).contains(index))
                .unwrap_or(start);
            for group in &mut projected[start..end] {
                group.stack_count = 0;
            }
            // The oldest member owns the original author boundary. Selection
            // changes only the front's details, never the underlying messages.
            projected[front] = groups[end - 1];
            projected[front].stack_count = end - start;
            fronts.push((items[front].clone(), end - start));
        }
        start = end;
    }
    if app.message_list_state.media_fronts != fronts {
        app.message_list_state.media_fronts = fronts;
        app.message_height_cache.mark_layout_changed();
        app.message_list_state.viewport_anchor = None;
        app.message_list_state.offset = 0;
        app.message_list_state.update_selected = selected.is_some();
    }
    projected
}

fn can_join(app: &App, newer: &wr::MessageId, older: &wr::MessageId) -> bool {
    let (Some(newer), Some(older)) = (app.messages.get(newer), app.messages.get(older)) else {
        return false;
    };
    let eligible = |message: &wr::Message| {
        !app.message_status(&message.info.id).deleted
            && matches!(
                &message.message,
                wr::MessageContent::File(file)
                    if matches!(file.kind, wr::FileKind::Image | wr::FileKind::Video)
            )
    };
    let day = |timestamp| {
        DateTime::from_timestamp(timestamp, 0)
            .map(DateTime::<Local>::from)
            .map(|time| time.date_naive())
    };
    eligible(newer)
        && eligible(older)
        && newer.info.is_from_me == older.info.is_from_me
        && newer.info.sender == older.info.sender
        && newer.info.timestamp >= older.info.timestamp
        && day(newer.info.timestamp).is_some()
        && day(newer.info.timestamp) == day(older.info.timestamp)
}
