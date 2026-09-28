use super::status_list::{StatusList, StatusListItem};
use crate::app::App;
use crate::app::actions::{FocusPane, StatusCompositionState};
use crate::ui::message_list::{render_own_status_messages, render_status_messages};
use crate::ui::{action_notice_text, conversation_areas, render_composer};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::Line,
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
    if app.status_composition != StatusCompositionState::Inactive {
        let authoring = app.status_composition != StatusCompositionState::Submitting;
        let navigating = app.status_composition == StatusCompositionState::Navigating;
        let inner_width = area.width.saturating_sub(2);
        let input_rows = if authoring {
            let width = inner_width.saturating_sub(2);
            let layout = crate::ui::layout::composer_visual_layout_with_direction(
                app.composer.input.lines(),
                width,
                app.composer_direction,
            );
            layout
                .row_count()
                .max(app.composer.visual_cursor(width, app.composer_direction).0 + 1)
        } else {
            2
        };
        let mut block =
            Block::bordered()
                .title(" Create status ")
                .border_style(Style::default().fg(if navigating {
                    Color::Green
                } else if authoring {
                    Color::Cyan
                } else {
                    Color::White
                }));
        if let Some(notice) = action_notice_text(app) {
            let available = (area.width as usize).saturating_sub(" Create status ".len() + 2);
            let notice = crate::ui::truncate_with_ellipsis(&notice, available);
            if !notice.is_empty() {
                block = block.title(Line::from(notice).right_aligned());
            }
        }
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let (statuses_area, composer_area) = conversation_areas(
            inner,
            input_rows,
            usize::from(authoring && app.composer.quote.is_some()),
            if authoring {
                app.composer.pending.len()
            } else {
                0
            },
        );
        if app.own_status_messages().is_empty() {
            frame.render_widget(Paragraph::new("No statuses published yet"), statuses_area);
        } else {
            render_own_status_messages(frame, app, statuses_area);
        }
        if authoring {
            render_composer(
                frame,
                app,
                composer_area,
                if inner_width < 45 {
                    if navigating {
                        " i Esc "
                    } else if app.composer.text().is_empty() {
                        " Enter Esc "
                    } else {
                        " Ctrl+O "
                    }
                } else if navigating {
                    " Status draft (navigation: i edit, Esc cancel) "
                } else {
                    " Status update (Enter publish, Esc navigate, Ctrl+O attach) "
                },
                None,
                None,
            );
        } else {
            let pending = app.pending_status_sends;
            let feedback = action_notice_text(app)
                .map(|notice| format!("{pending} updates pending\n{notice}"))
                .unwrap_or_else(|| format!("{pending} updates pending"));
            render_composer(
                frame,
                app,
                composer_area,
                " Publishing status ",
                None,
                Some(&feedback),
            );
        }
        return;
    }

    let title = app
        .open_status_contact()
        .map(|contact| app.contact_name(&contact).to_string())
        .unwrap_or_else(|| "Status".to_string());
    let border_color = if app.focus_pane == FocusPane::Conversation {
        Color::Green
    } else {
        Color::White
    };
    let mut block = Block::bordered()
        .title(title)
        .border_style(Style::default().fg(border_color));
    if let Some(notice) = action_notice_text(app) {
        block = block.title(Line::from(notice).right_aligned());
    }
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
