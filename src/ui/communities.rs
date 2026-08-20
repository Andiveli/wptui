use std::sync::Arc;

use crate::app::actions::FocusPane;
use crate::app::contact_avatars::prioritized_avatar_requests;
use crate::app::{App, CommunityNavigationRow};
use crate::ui::contact_list::{
    AVATAR_HEIGHT, AVATAR_WIDTH, CONTACT_ITEM_HEIGHT, initials, truncate,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Paragraph, StatefulWidget},
};

fn community_layout(
    rows: &[CommunityNavigationRow],
    selected: Option<usize>,
    area: Rect,
) -> Vec<(CommunityNavigationRow, Rect, Option<usize>)> {
    if area.is_empty() {
        return Vec::new();
    }

    let mut logical_rows = Vec::new();
    let mut selectable_index = 0;
    for row in rows {
        let selected_index = (!matches!(row, CommunityNavigationRow::Separator)).then(|| {
            let index = selectable_index;
            selectable_index += 1;
            index
        });
        logical_rows.push((row.clone(), selected_index));
    }

    let capacity = usize::from(area.height);
    let heights = logical_rows
        .iter()
        .map(|(row, _)| {
            if matches!(row, CommunityNavigationRow::Separator) {
                1
            } else {
                CONTACT_ITEM_HEIGHT
            }
        })
        .collect::<Vec<_>>();
    let selected_row = selected.and_then(|selection| {
        logical_rows
            .iter()
            .position(|(_, index)| *index == Some(selection))
    });
    let mut start = selected_row.unwrap_or(0);
    while start > 0
        && heights[start..=selected_row.unwrap_or(start)]
            .iter()
            .sum::<usize>()
            < capacity
    {
        start -= 1;
    }
    while heights[start..]
        .iter()
        .scan(0, |used, height| {
            *used += *height;
            Some(*used)
        })
        .any(|used| used > capacity)
    {
        if start == selected_row.unwrap_or(start) {
            break;
        }
        start += 1;
    }

    logical_rows
        .into_iter()
        .enumerate()
        .skip(start)
        .scan(area.y, |y, (index, (row, selectable_index))| {
            if *y >= area.bottom() {
                return None;
            }
            let height = heights[index].min(usize::from(area.bottom().saturating_sub(*y))) as u16;
            let result = (
                row,
                Rect::new(area.x, *y, area.width, height),
                selectable_index,
            );
            *y = y.saturating_add(heights[index] as u16);
            Some(result)
        })
        .collect()
}

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::bordered()
        .title("Communities")
        .border_style(
            Style::default().fg(if app.focus_pane == FocusPane::ChatList {
                Color::Green
            } else {
                Color::White
            }),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if app.communities_unavailable {
        frame.render_widget(Paragraph::new("Community data unavailable"), inner);
        return;
    }
    let rows = app.community_navigation_rows();
    if rows.is_empty() {
        frame.render_widget(Paragraph::new("No communities"), inner);
        return;
    }

    let selected = app.chat_list_state.selected();
    let targets = rows
        .iter()
        .filter_map(|row| match row {
            CommunityNavigationRow::Root(jid) | CommunityNavigationRow::Group(jid) => {
                Some(jid.clone())
            }
            CommunityNavigationRow::ViewAll(_) | CommunityNavigationRow::Separator => None,
        })
        .collect::<Vec<_>>();
    app.contact_avatars.clear_window();
    app.contact_avatars.schedule(
        prioritized_avatar_requests(&targets, None, 0, targets.len()),
        app.tx.clone(),
        Arc::clone(&app.picker),
    );
    for (row, row_area, selectable_index) in community_layout(&rows, selected, inner) {
        let selected = selectable_index == selected;
        let base = if selected {
            Style::default().fg(Color::Green).bg(Color::DarkGray)
        } else {
            Style::default()
        };
        frame.buffer_mut().set_style(row_area, base);
        let is_root = matches!(&row, CommunityNavigationRow::Root(_));
        let (name, target, separator) = match &row {
            CommunityNavigationRow::Root(jid) | CommunityNavigationRow::Group(jid) => {
                let node = app.communities.iter().find(|node| node.jid == *jid);
                (
                    node.map(|node| node.name.clone())
                        .unwrap_or_else(|| jid.0.to_string()),
                    Some(jid.clone()),
                    false,
                )
            }
            CommunityNavigationRow::ViewAll(jid) => ("View all".into(), Some(jid.clone()), false),
            CommunityNavigationRow::Separator => ("────────────────".into(), None, true),
        };
        let style = if is_root {
            base.add_modifier(Modifier::BOLD)
        } else {
            base
        };
        if separator {
            frame.buffer_mut().set_stringn(
                row_area.x,
                row_area.y,
                &name,
                row_area.width as usize,
                style,
            );
            continue;
        }
        let avatar_area = Rect::new(
            row_area.x,
            row_area.y,
            AVATAR_WIDTH.min(row_area.width),
            AVATAR_HEIGHT.min(row_area.height),
        );
        if avatar_area.width == AVATAR_WIDTH
            && avatar_area.height == AVATAR_HEIGHT
            && let Some(target) = target.as_ref()
            && let Some(protocol) = app.contact_avatars.protocol_mut(target)
        {
            ratatui_image::StatefulImage::default().render(
                avatar_area,
                frame.buffer_mut(),
                protocol,
            );
        } else {
            frame.buffer_mut().set_stringn(
                row_area.x,
                row_area.y,
                &truncate(&initials(&name), AVATAR_WIDTH as usize),
                AVATAR_WIDTH as usize,
                style,
            );
        }
        frame.buffer_mut().set_stringn(
            row_area.x.saturating_add(AVATAR_WIDTH + 1),
            row_area.y,
            &name,
            row_area.width.saturating_sub(AVATAR_WIDTH + 1) as usize,
            style,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::community_layout;
    use crate::app::CommunityNavigationRow;
    use ratatui::layout::Rect;
    use whatsrust::JID;

    fn rows(group_count: usize) -> Vec<CommunityNavigationRow> {
        std::iter::once(CommunityNavigationRow::Root(JID::from(
            "root@g.us".to_owned(),
        )))
        .chain(
            (0..group_count).map(|index| {
                CommunityNavigationRow::Group(JID::from(format!("group-{index}@g.us")))
            }),
        )
        .chain([
            CommunityNavigationRow::ViewAll(JID::from("root@g.us".to_owned())),
            CommunityNavigationRow::Separator,
        ])
        .collect()
    }

    #[test]
    fn community_blocks_are_flat() {
        let layout = community_layout(&rows(2), Some(0), Rect::new(0, 0, 40, 13));
        assert_eq!(layout.len(), 5);
        assert!(matches!(layout[0].0, CommunityNavigationRow::Root(_)));
        assert!(matches!(layout[1].0, CommunityNavigationRow::Group(_)));
        assert_eq!(layout[1].2, Some(1));
        assert!(matches!(layout[3].0, CommunityNavigationRow::ViewAll(_)));
    }

    #[test]
    fn selected_group_is_visible_and_rows_stay_bounded() {
        let layout = community_layout(&rows(6), Some(5), Rect::new(0, 0, 5, 6));
        assert!(layout.iter().any(|row| row.2 == Some(5)));
        assert!(layout.iter().all(|row| row.1.right() <= 5));
    }
}
