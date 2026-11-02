//! Presentation-only stacks keep every original ID and navigation index.
use chrono::{DateTime, Local};
use whatsrust as wr;

use crate::app::{App, FileMeta, Metadata};

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
    let mut members = Vec::new();
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
            members.push((items[front].clone(), items[start..end].to_vec()));
        }
        start = end;
    }
    app.message_list_state.media_members = members;
    if app.message_list_state.media_fronts != fronts {
        app.message_list_state.media_fronts = fronts;
        app.message_height_cache.mark_layout_changed();
        app.message_list_state.viewport_anchor = None;
        app.message_list_state.offset = 0;
        app.message_list_state.update_selected = selected.is_some();
    }
    projected
}

/// All native payloads are prepared for their final visible rectangle. Never
/// copy a clipped Kitty/Sixel escape from an offscreen message buffer.
pub(super) fn render_region(
    app: &mut App,
    front: &wr::MessageId,
    area: ratatui::layout::Rect,
    viewport: ratatui::layout::Rect,
    y_offset: isize,
    buf: &mut ratatui::buffer::Buffer,
    plan: &mut crate::app::events::MediaRenderPlan,
) {
    use ratatui::layout::{Rect, Size};

    let Some((_, members)) = app
        .message_list_state
        .media_members
        .iter()
        .find(|(id, _)| id == front)
    else {
        return;
    };
    let members = members.clone();
    let Some(position) = members.iter().position(|id| id == front) else {
        return;
    };
    let font = app.picker.lock().unwrap().font_size();
    let available = Size::new(area.width, area.height);
    let mut x = area.x;
    // A cyclic ordered window moves one member per vertical navigation step;
    // reversing selection restores the exact previous order and geometry.
    for step in 0..members.len().min(4) {
        let id = &members[(position + step) % members.len()];
        let Some(wr::MessageContent::File(file)) = app.messages.get(id).map(|m| &m.message) else {
            continue;
        };
        let file = file.clone();
        let loaded = matches!(app.metadata.get(id), Some(Metadata::File(FileMeta::Loaded)));
        let canvas = app
            .stack_previews
            .fit_for(&file.path, font, available)
            .unwrap_or(Size::new(available.width.min(36), available.height));
        if canvas.width == 0 || canvas.height == 0 || x >= area.right() {
            break;
        }
        let width = if step == 0 {
            canvas.width
        } else {
            canvas.width.min(4).min(area.right() - x)
        };
        let image_y = y_offset + area.y as isize;
        let visible_top = image_y.max(viewport.top() as isize);
        let visible_bottom = (image_y + canvas.height as isize).min(viewport.bottom() as isize);
        if visible_bottom > visible_top {
            let visible = Rect::new(
                x,
                visible_top as u16,
                width,
                (visible_bottom - visible_top) as u16,
            )
            .intersection(viewport);
            super::message_media::request_preview(app, id, &file, plan);
            let crop = Rect::new(
                if step == 0 { 0 } else { canvas.width - width },
                (visible_top - image_y) as u16,
                visible.width,
                visible.height,
            );
            if loaded
                && let Some(key) = app.stack_previews.key(&file.path, canvas, crop)
            {
                if app.stack_previews.request(key.clone()) {
                    // Reuse the existing post-draw, ID-deduplicated media effect.
                    // Loaded files prepare queued crops rather than decoding again.
                    plan.append(crate::app::events::MediaRenderEffect::LoadFilePreview(id.clone()));
                }
                if let Some(protocol) = app.stack_previews.get(&key) {
                    ratatui::widgets::Widget::render(
                        ratatui_image::Image::new(protocol),
                        visible,
                        buf,
                    );
                    if step == 0 {
                        app.mouse_hit_map.media.push(crate::app::mouse_input::MediaHit {
                            area: visible,
                            id: front.clone(),
                            kind: file.kind.clone(),
                        });
                    }
                }
                app.touch_image_cache(&file.path);
            }
        }
        x = x.saturating_add(width).saturating_add(1);
    }
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
