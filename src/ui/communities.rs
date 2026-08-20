use crate::app::App;
use crate::app::actions::FocusPane;
use crate::ui::contact_list::initials;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Paragraph},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommunityRowKind {
    Root,
    Separator,
    Child,
    ViewAll,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CommunityRow {
    node_index: usize,
    selectable_index: Option<usize>,
    kind: CommunityRowKind,
    area: Rect,
    text: Rect,
}

fn community_layout(
    communities: &[crate::app::CommunityNode],
    selected: Option<usize>,
    area: Rect,
) -> Vec<CommunityRow> {
    if area.is_empty() {
        return Vec::new();
    }

    let mut logical_rows = Vec::new();
    let mut selectable_index = 0;
    for (node_index, root) in communities
        .iter()
        .enumerate()
        .filter(|(_, node)| node.is_root)
    {
        logical_rows.push((node_index, None, CommunityRowKind::Root));
        logical_rows.push((node_index, None, CommunityRowKind::Separator));
        for (node_index, _) in communities
            .iter()
            .enumerate()
            .filter(|(_, node)| !node.is_root && root.linked_groups.contains(&node.jid))
        {
            logical_rows.push((node_index, Some(selectable_index), CommunityRowKind::Child));
            selectable_index += 1;
        }
        logical_rows.push((node_index, None, CommunityRowKind::ViewAll));
        logical_rows.push((node_index, None, CommunityRowKind::Separator));
    }

    let capacity = usize::from(area.height);
    let selected_row = selected.and_then(|selection| {
        logical_rows
            .iter()
            .position(|(_, selectable, _)| *selectable == Some(selection))
    });
    let start = selected_row
        .map(|row| row.saturating_sub(capacity.saturating_sub(1)))
        .unwrap_or(0)
        .min(logical_rows.len().saturating_sub(capacity));

    logical_rows
        .into_iter()
        .skip(start)
        .take(capacity)
        .enumerate()
        .map(|(visible_index, (node_index, selectable_index, kind))| {
            let y = area.y.saturating_add(visible_index as u16);
            let text_x = match kind {
                CommunityRowKind::Root
                | CommunityRowKind::Separator
                | CommunityRowKind::ViewAll => area.x,
                CommunityRowKind::Child => area.x.saturating_add(2).min(area.right()),
            };
            CommunityRow {
                node_index,
                selectable_index,
                kind,
                area: Rect::new(area.x, y, area.width, 1),
                text: Rect::new(text_x, y, area.right().saturating_sub(text_x), 1),
            }
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
    let mut communities = Vec::new();
    for root in app.communities.iter().filter(|n| n.is_root) {
        let mut root = root.clone();
        root.linked_groups.retain(|jid| app.chats.contains_key(jid));
        if root.linked_groups.is_empty() {
            continue;
        }
        communities.push(root.clone());
        communities.extend(
            app.communities
                .iter()
                .filter(|n| {
                    !n.is_root
                        && root.linked_groups.contains(&n.jid)
                        && app.chats.contains_key(&n.jid)
                })
                .cloned(),
        );
    }
    if communities.is_empty() {
        frame.render_widget(Paragraph::new("No communities"), inner);
        return;
    }

    let selected = app.chat_list_state.selected();
    for row in community_layout(&communities, selected, inner) {
        let node = &communities[row.node_index];
        let selected = row.selectable_index == selected;
        let base = if selected {
            Style::default().fg(Color::Green).bg(Color::DarkGray)
        } else {
            Style::default()
        };
        frame.buffer_mut().set_style(row.area, base);

        let text = match row.kind {
            CommunityRowKind::Root => format!("{}  {}", initials(&node.name), node.name),
            CommunityRowKind::Separator => "────────".into(),
            CommunityRowKind::Child => node.name.clone(),
            CommunityRowKind::ViewAll => "View all".into(),
        };
        let style = if row.kind == CommunityRowKind::Root {
            base.add_modifier(Modifier::BOLD)
        } else {
            base
        };
        frame.buffer_mut().set_stringn(
            row.text.x,
            row.text.y,
            &text,
            row.text.width as usize,
            style,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{CommunityRowKind, community_layout};
    use crate::app::CommunityNode;
    use ratatui::layout::Rect;
    use whatsrust::JID;

    fn nodes(group_count: usize) -> Vec<CommunityNode> {
        let groups = (0..group_count)
            .map(|index| JID::from(format!("group-{index}@g.us")))
            .collect::<Vec<_>>();
        let mut result = vec![CommunityNode {
            jid: JID::from("root@g.us".to_owned()),
            name: "Community".into(),
            is_root: true,
            linked_groups: groups.clone(),
            is_joined: true,
            is_default_subgroup: false,
            is_announce: None,
            participant_count: None,
        }];
        result.extend(
            groups
                .into_iter()
                .enumerate()
                .map(|(index, jid)| CommunityNode {
                    jid,
                    name: format!("Group {index}"),
                    is_root: false,
                    linked_groups: Vec::new(),
                    is_joined: true,
                    is_default_subgroup: false,
                    is_announce: None,
                    participant_count: None,
                }),
        );
        result
    }

    #[test]
    fn community_blocks_are_flat() {
        let layout = community_layout(&nodes(2), Some(0), Rect::new(0, 0, 40, 12));
        assert_eq!(layout.len(), 6);
        assert_eq!(layout[0].kind, CommunityRowKind::Root);
        assert_eq!(layout[1].kind, CommunityRowKind::Separator);
        assert_eq!(layout[2].kind, CommunityRowKind::Child);
        assert_eq!(layout[2].selectable_index, Some(0));
        assert_eq!(layout[4].kind, CommunityRowKind::ViewAll);
    }

    #[test]
    fn selected_group_is_visible_and_rows_stay_bounded() {
        let layout = community_layout(&nodes(6), Some(5), Rect::new(0, 0, 5, 3));
        assert!(layout.iter().any(|row| row.selectable_index == Some(5)));
        assert!(
            layout
                .iter()
                .all(|row| row.area.right() <= 5 && row.text.right() <= 5)
        );
    }
}
