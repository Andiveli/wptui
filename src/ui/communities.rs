use std::sync::Arc;

use crate::app::actions::FocusPane;
use crate::app::contact_avatars::{AvatarTarget, prioritized_avatar_requests};
use crate::app::{App, CommunityNavigationRow, community_hierarchy::community_group_label};
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

fn community_avatar_targets(_app: &App, rows: &[CommunityNavigationRow]) -> Vec<AvatarTarget> {
    rows.iter()
        .filter_map(|row| match row {
            CommunityNavigationRow::Root(jid) => Some(AvatarTarget::CommunityRoot(jid.clone())),
            CommunityNavigationRow::Group(jid) => Some(AvatarTarget::Contact(jid.clone())),
            CommunityNavigationRow::Announcement(_)
            | CommunityNavigationRow::ViewAll(_)
            | CommunityNavigationRow::Separator => None,
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
    let targets = community_avatar_targets(app, &rows);
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
            CommunityNavigationRow::Root(jid) => {
                let node = app.communities.iter().find(|node| node.jid == *jid);
                (
                    node.map(|node| node.name.clone())
                        .unwrap_or_else(|| jid.0.to_string()),
                    Some(AvatarTarget::CommunityRoot(jid.clone())),
                    false,
                )
            }
            CommunityNavigationRow::Group(jid) => {
                let node = app.communities.iter().find(|node| node.jid == *jid);
                let name = node
                    .map(community_group_label)
                    .unwrap_or_else(|| jid.0.to_string());
                (
                    name.clone(),
                    Some(AvatarTarget::Contact(jid.clone())),
                    false,
                )
            }
            CommunityNavigationRow::Announcement(_) => ("Announcements".into(), None, false),
            CommunityNavigationRow::ViewAll(_) => ("View all".into(), None, false),
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
            let fallback = match &row {
                CommunityNavigationRow::ViewAll(_) => ">".to_owned(),
                CommunityNavigationRow::Announcement(_) => "📢".to_owned(),
                _ => initials(&name),
            };
            frame.buffer_mut().set_stringn(
                row_area.x,
                row_area.y,
                &truncate(&fallback, AVATAR_WIDTH as usize),
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
    use super::{community_avatar_targets, community_layout, render};
    use crate::app::contact_avatars::AvatarTarget;
    use crate::app::{Chat, CommunityNavigationRow, CommunityNode, test_support::TestApp};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
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

    #[test]
    fn avatar_targets_exclude_symbolic_view_all() {
        let mut app = TestApp::new();
        app.communities = vec![CommunityNode {
            jid: JID::from("root@g.us".to_owned()),
            name: "Community".into(),
            is_root: true,
            linked_groups: vec![
                JID::from("group-0@g.us".to_owned()),
                JID::from("group-1@g.us".to_owned()),
            ],
            is_joined: true,
            is_default_subgroup: false,
            is_announce: None,
            participant_count: None,
        }];
        let targets = community_avatar_targets(&app, &rows(2));
        assert_eq!(targets.len(), 3);
        assert!(matches!(targets[0], AvatarTarget::CommunityRoot(_)));
    }

    fn community_app(child_announce: Option<bool>) -> TestApp {
        let mut test_app = TestApp::new();
        let root = JID::from("root@g.us".to_owned());
        let group = JID::from("group@g.us".to_owned());
        test_app.chats.insert(
            group.clone(),
            Chat {
                jid: group.clone(),
                last_message_time: None,
            },
        );
        test_app.communities = vec![
            CommunityNode {
                jid: root.clone(),
                name: "Community".into(),
                is_root: true,
                linked_groups: vec![group.clone()],
                is_joined: true,
                is_default_subgroup: false,
                is_announce: None,
                participant_count: None,
            },
            CommunityNode {
                jid: group,
                name: "Group".into(),
                is_root: false,
                linked_groups: Vec::new(),
                is_joined: true,
                is_default_subgroup: false,
                is_announce: child_announce,
                participant_count: None,
            },
        ];
        test_app
    }

    #[test]
    fn main_community_render_labels_announcement_children() {
        let mut app = community_app(Some(true));
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();

        terminal
            .draw(|frame| render(frame, &mut app, frame.area()))
            .unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Announcements"), "{rendered}");
        assert!(rendered.contains("📢"), "{rendered}");
        assert!(rendered.contains(">"), "{rendered}");
        assert!(!rendered.contains("Group"), "{rendered}");
    }
}
