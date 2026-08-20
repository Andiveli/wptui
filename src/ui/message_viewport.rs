use std::cmp::{max, min};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Paragraph, Widget},
};
use whatsrust::{self as wr, FileKind};

use crate::app::runtime_diagnostics::MessageListCounts;
use crate::app::{App, FileMeta, Metadata};

use super::message_formatting::unread_divider_line;
use super::message_list_state::ViewportAnchor;
use super::{
    AuthorGroupContext, message_height, preview_height, render_message, spacing_after_message,
};

pub(super) fn render(
    frame: &mut ratatui::Frame,
    app: &mut App,
    list_area: Rect,
    items: &[wr::Message],
    author_groups: &[AuthorGroupContext],
    unread_count: usize,
    width: isize,
    start_index: usize,
    mut y: isize,
    counts: &mut MessageListCounts,
) -> Option<ViewportAnchor> {
    let divider_after = unread_count.checked_sub(1);
    let mut viewport_anchor = None;

    for (i, item) in items.iter().enumerate().skip(start_index) {
        let is_selected = app.message_list_state.selected == Some(i);
        let author_group = author_groups[i];
        let height = message_height(item, width as usize, is_selected, author_group, app) as isize;

        let bottom = y;
        let top = y - height;

        if bottom <= list_area.top() as isize {
            break;
        }

        let Some(top_i64) = i64::try_from(top).ok() else {
            break;
        };
        let Some(bottom_i64) = i64::try_from(bottom).ok() else {
            break;
        };
        if crate::app::read_receipts::intersects(
            top_i64,
            bottom_i64,
            i64::from(list_area.top()),
            i64::from(list_area.bottom()),
        ) {
            counts.visible_rows = counts.visible_rows.saturating_add(1);
            counts.receipt_candidates = counts.receipt_candidates.saturating_add(1);
            viewport_anchor.get_or_insert((i, y));
            app.observe_visible_message(item, true);
            let too_low = top < list_area.top() as isize;
            let too_high = bottom > list_area.bottom() as isize;

            if too_low || too_high {
                let item_area = Rect::new(0, 0, width as u16, height as u16);
                let mut buf = Buffer::empty(item_area);

                let available_top = max(top, list_area.top() as isize) as u16;
                let available_bottom = min(bottom, list_area.bottom() as isize) as u16;
                let visible_buf_top = (available_top as isize - top) as u16;
                let visible_buf_height = available_bottom - available_top;
                counts.temporary_buffer_rows = counts
                    .temporary_buffer_rows
                    .saturating_add(u64::from(visible_buf_height));

                let render_image = match &item.message {
                    wr::MessageContent::File(data)
                        if matches!(
                            app.metadata.get(&item.info.id),
                            Some(Metadata::File(FileMeta::Loaded))
                        ) && matches!(
                            data.kind,
                            FileKind::Image | FileKind::Sticker | FileKind::Video
                        ) =>
                    {
                        counts.media_rows = counts.media_rows.saturating_add(1);
                        let image_top = u16::from(is_selected || author_group.starts_group())
                            + u16::from(item.info.quote_id.is_some());
                        let image_bottom = image_top + preview_height(&data.kind) as u16;
                        let visible_buf_bottom = visible_buf_top + visible_buf_height;
                        visible_buf_top < image_bottom && visible_buf_bottom > image_top
                    }
                    _ => true,
                };

                render_message(
                    &mut buf,
                    item,
                    is_selected,
                    author_group,
                    app,
                    item_area,
                    render_image,
                );

                let buf_area = Rect::new(
                    list_area.left(),
                    available_top,
                    width as u16,
                    visible_buf_height,
                );

                if !buf_area.is_empty() {
                    let mut mapped_area = buf_area;
                    mapped_area.y = visible_buf_top;
                    mapped_area.x = 0;

                    let (inject_transmit, media_first_row, media_first_col) = match &item.message {
                        wr::MessageContent::File(data)
                            if matches!(
                                app.metadata.get(&item.info.id),
                                Some(Metadata::File(FileMeta::Loaded))
                            ) && matches!(
                                data.kind,
                                FileKind::Image | FileKind::Sticker | FileKind::Video
                            ) =>
                        {
                            let first_row = u16::from(is_selected || author_group.starts_group())
                                + u16::from(item.info.quote_id.is_some());
                            let inject = mapped_area.y > first_row
                                && mapped_area.y < first_row + preview_height(&data.kind) as u16;
                            (inject, first_row, if is_selected { 2 } else { 0 })
                        }
                        _ => (false, 0, 0),
                    };

                    for (row_idx, (screen_row, msg_row)) in
                        buf_area.rows().zip(mapped_area.rows()).enumerate()
                    {
                        for (screen_col, msg_col) in screen_row.columns().zip(msg_row.columns()) {
                            let mut cell = buf[msg_col].clone();
                            if inject_transmit
                                && row_idx == 0
                                && screen_col.x == list_area.left() + media_first_col
                            {
                                let first_sym = buf[(media_first_col, media_first_row)].symbol();
                                if let Some(pos) = first_sym.find("\x1b[s") {
                                    let merged = format!("{}{}", &first_sym[..pos], cell.symbol());
                                    cell.set_symbol(&merged);
                                }
                            }
                            frame.buffer_mut()[screen_col] = cell;
                        }
                    }
                }
            } else {
                if matches!(
                    &item.message,
                    wr::MessageContent::File(data)
                        if matches!(
                            app.metadata.get(&item.info.id),
                            Some(Metadata::File(FileMeta::Loaded))
                        ) && matches!(data.kind, FileKind::Image | FileKind::Sticker | FileKind::Video)
                ) {
                    counts.media_rows = counts.media_rows.saturating_add(1);
                }
                let item_area = Rect {
                    x: list_area.left(),
                    y: top as u16,
                    width: width as u16,
                    height: height as u16,
                };

                render_message(
                    frame.buffer_mut(),
                    item,
                    is_selected,
                    author_group,
                    app,
                    item_area,
                    true,
                );
            }
        }

        y -= height
            + spacing_after_message(i, author_groups, app.message_list_state.selected) as isize;
        if divider_after == Some(i) {
            y -= 1;
            if y >= list_area.top() as isize && y < list_area.bottom() as isize {
                Paragraph::new(unread_divider_line(list_area.width as usize)).render(
                    Rect::new(list_area.left(), y as u16, width as u16, 1),
                    frame.buffer_mut(),
                );
            }
        }
    }

    viewport_anchor.and_then(|(index, y)| {
        items.get(index).map(|item| ViewportAnchor {
            index,
            y,
            width: width as usize,
            offset: app.message_list_state.offset,
            generation: app.message_height_cache.generation(),
            message_id: item.info.id.clone(),
            bottom: list_area.bottom(),
        })
    })
}
