use crate::app::App;
use crate::app::actions::FocusPane;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Paragraph},
};

const BRANCH_WIDTH: u16 = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
struct CommunityRow {
    node_index: usize,
    selectable_index: Option<usize>,
    area: Rect,
    branch: Rect,
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
        logical_rows.push((node_index, None));
        for (node_index, _) in communities
            .iter()
            .enumerate()
            .filter(|(_, node)| !node.is_root && root.linked_groups.contains(&node.jid))
        {
            logical_rows.push((node_index, Some(selectable_index)));
            selectable_index += 1;
        }
    }

    let capacity = usize::from(area.height);
    let selected_row = selected.and_then(|selection| {
        logical_rows
            .iter()
            .position(|(_, selectable)| *selectable == Some(selection))
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
        .map(|(visible_index, (node_index, selectable_index))| {
            let y = area.y.saturating_add(visible_index as u16);
            let row_area = Rect::new(area.x, y, area.width, 1);
            let branch_x = area.x.min(area.right());
            let branch = Rect::new(
                branch_x,
                y,
                BRANCH_WIDTH.min(area.right().saturating_sub(branch_x)),
                1,
            );
            let text_x = if selectable_index.is_some() {
                branch_x.saturating_add(BRANCH_WIDTH).min(area.right())
            } else {
                area.x
            };
            let text = Rect::new(text_x, y, area.right().saturating_sub(text_x), 1);
            CommunityRow {
                node_index,
                selectable_index,
                area: row_area,
                branch,
                text,
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
    if app.communities.is_empty() {
        frame.render_widget(Paragraph::new("No communities"), inner);
        return;
    }

    let selected = app.chat_list_state.selected();
    for row in community_layout(&app.communities, selected, inner) {
        let node = &app.communities[row.node_index];
        let selected = row.selectable_index == selected;
        let base = if selected {
            Style::default().fg(Color::Green).bg(Color::DarkGray)
        } else {
            Style::default()
        };
        frame.buffer_mut().set_style(row.area, base);

        if row.selectable_index.is_some() {
            frame.buffer_mut().set_stringn(
                row.branch.x,
                row.branch.y,
                "├─",
                row.branch.width as usize,
                base,
            );
        }
        let text_style = if row.selectable_index.is_none() {
            base.add_modifier(Modifier::BOLD)
        } else {
            base
        };
        frame.buffer_mut().set_stringn(
            row.text.x,
            row.text.y,
            &node.name,
            row.text.width as usize,
            text_style,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::community_layout;
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
                }),
        );
        result
    }

    #[test]
    fn root_is_rendered_once_and_groups_are_compact_branch_rows() {
        let layout = community_layout(&nodes(2), Some(0), Rect::new(0, 0, 40, 12));
        assert_eq!(layout.len(), 3);
        assert_eq!(layout[0].selectable_index, None);
        assert_eq!(layout[0].text.y, 0);
        assert_eq!(layout[1].selectable_index, Some(0));
        assert_eq!(layout[1].text.y, 1);
        assert_eq!(layout[2].selectable_index, Some(1));
        assert_eq!(layout[2].text.y, 2);
        assert_eq!(layout[1].text.x, layout[1].branch.right());
    }

    #[test]
    fn selected_group_is_visible_and_root_is_not_selectable() {
        let layout = community_layout(&nodes(6), Some(5), Rect::new(0, 0, 40, 3));
        assert!(layout.iter().any(|row| row.selectable_index == Some(5)));
        assert!(layout.iter().all(|row| row.selectable_index.is_some()));
    }

    #[test]
    fn narrow_and_short_areas_remain_bounded() {
        let layout = community_layout(&nodes(2), Some(0), Rect::new(0, 0, 5, 1));
        assert!(
            layout
                .iter()
                .all(|row| row.area.right() <= 5 && row.area.bottom() <= 1)
        );
        assert!(
            layout
                .iter()
                .all(|row| row.branch.right() <= 5 && row.text.right() <= 5)
        );
    }
}
