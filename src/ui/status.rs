use super::status_list::{StatusList, StatusListItem};
use crate::app::actions::{FocusPane, StatusCompositionState};
use crate::app::{App, read_receipts::VisibilityPlan};
use crate::ui::message_list::{render_own_status_messages, render_status_messages_with_plan};
use crate::ui::{action_notice_text, conversation_areas, render_composer};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
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

pub(super) fn render_statuses_with_plan(
    frame: &mut Frame,
    app: &mut App,
    media_render_plan: &mut crate::app::events::MediaRenderPlan,
    visibility_plan: &mut VisibilityPlan,
    area: Rect,
) {
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
        let (mut statuses_area, composer_area) = conversation_areas(
            inner,
            input_rows,
            usize::from(authoring && app.composer.quote.is_some()),
            if authoring {
                app.composer.pending.len()
            } else {
                0
            },
        );
        if app.status_retry_warning {
            let [list, warning] =
                Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(statuses_area);
            statuses_area = list;
            let message = if warning.width < 20 {
                "MAYBE SENT\nCHECK RETRY"
            } else {
                "MAY BE PUBLISHED\nCHECK BEFORE RETRY"
            };
            frame.render_widget(
                Paragraph::new(message).style(Style::default().fg(Color::Yellow)),
                warning,
            );
        }
        if app.own_status_messages().is_empty() && app.pending_outgoing_status.is_empty() {
            frame.render_widget(Paragraph::new("No statuses published yet"), statuses_area);
        } else {
            render_own_status_messages(
                frame,
                app,
                media_render_plan,
                visibility_plan,
                statuses_area,
            );
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
                    " Status draft (navigation: i edit, d delete selected published status, Esc cancel) "
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
    render_status_messages_with_plan(frame, app, media_render_plan, visibility_plan, content_area);
}

#[cfg(test)]
mod tests {
    use ratatui::{
        Terminal,
        backend::TestBackend,
        layout::Rect,
        style::{Color, Modifier},
    };
    use whatsrust as wr;

    use super::render_statuses_with_plan;
    use crate::app::actions::{ConversationMode, FocusPane, Section, StatusCompositionState};
    use crate::app::read_receipts::VisibilityPlan;
    use crate::app::status_projection::STATUS_BROADCAST_CHAT;
    use crate::app::test_support::TestApp;

    #[test]
    fn status_composer_border_follows_its_own_focus_mode() {
        for (mode, expected) in [
            (StatusCompositionState::Authoring, Color::Cyan),
            (StatusCompositionState::Navigating, Color::Green),
            (StatusCompositionState::Submitting, Color::White),
        ] {
            let mut app = TestApp::new();
            app.selected_section = Section::Status;
            app.focus_pane = FocusPane::ChatList;
            app.conversation_mode = ConversationMode::MessageNavigation;
            app.status_composition = mode;
            let mut terminal = Terminal::new(TestBackend::new(60, 18)).unwrap();
            let mut media_plan = crate::app::events::MediaRenderPlan::default();
            let mut visibility_plan = VisibilityPlan::default();
            terminal
                .draw(|frame| {
                    render_statuses_with_plan(
                        frame,
                        &mut app,
                        &mut media_plan,
                        &mut visibility_plan,
                        Rect::new(0, 0, 60, 18),
                    )
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            let composer_corner = (1..18)
                .find_map(|y| {
                    let cell = &buffer[(1, y)];
                    (cell.symbol() == "╭").then_some(cell)
                })
                .expect("own-status composer border must be visible");
            assert_eq!(composer_corner.fg, expected, "status mode {mode:?}");
        }
    }

    #[test]
    fn own_status_changes_from_subdued_pending_row_to_confirmed_row() {
        let mut app = TestApp::new();
        app.status_composition = StatusCompositionState::Submitting;
        app.pending_outgoing_status
            .push((42, wr::MessageContent::Text("QXYZ".into())));
        let mut terminal = Terminal::new(TestBackend::new(60, 18)).unwrap();
        let mut media_plan = crate::app::events::MediaRenderPlan::default();
        let mut visibility_plan = VisibilityPlan::default();
        terminal
            .draw(|frame| {
                render_statuses_with_plan(
                    frame,
                    &mut app,
                    &mut media_plan,
                    &mut visibility_plan,
                    Rect::new(0, 0, 60, 18),
                )
            })
            .unwrap();
        let pending = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .find(|cell| cell.symbol() == "Q")
            .expect("pending row");
        assert!(pending.modifier.contains(Modifier::DIM));

        let confirmed = wr::Message {
            info: wr::MessageInfo {
                id: "canonical-qxyz".into(),
                chat: STATUS_BROADCAST_CHAT.to_owned().into(),
                sender: "self@s.whatsapp.net".to_owned().into(),
                mentions_self: false,
                timestamp: 100,
                is_from_me: true,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text("QXYZ".into()),
        };
        assert!(app.complete_status_send(42, confirmed));
        app.status_composition = StatusCompositionState::Navigating;
        let mut media_plan = crate::app::events::MediaRenderPlan::default();
        let mut visibility_plan = VisibilityPlan::default();
        terminal
            .draw(|frame| {
                render_statuses_with_plan(
                    frame,
                    &mut app,
                    &mut media_plan,
                    &mut visibility_plan,
                    Rect::new(0, 0, 60, 18),
                )
            })
            .unwrap();
        let cells = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .filter(|cell| cell.symbol() == "Q")
            .collect::<Vec<_>>();
        assert_eq!(
            cells.len(),
            1,
            "one confirmed row without an optimistic duplicate"
        );
        assert!(!cells[0].modifier.contains(Modifier::DIM));
    }

    #[test]
    fn queued_status_media_and_caption_are_subdued_without_download() {
        for kind in [wr::FileKind::Image, wr::FileKind::Video] {
            let mut app = TestApp::new();
            app.status_composition = StatusCompositionState::Submitting;
            app.pending_outgoing_status.push((
                42,
                wr::MessageContent::File(wr::FileContent {
                    kind,
                    path: "Qmedia.png".into(),
                    caption: Some("Q caption".into()),
                    ..Default::default()
                }),
            ));
            let mut terminal = Terminal::new(TestBackend::new(60, 24)).unwrap();
            let mut media_plan = crate::app::events::MediaRenderPlan::default();
            let mut visibility_plan = VisibilityPlan::default();
            terminal
                .draw(|frame| {
                    render_statuses_with_plan(
                        frame,
                        &mut app,
                        &mut media_plan,
                        &mut visibility_plan,
                        Rect::new(0, 0, 60, 24),
                    )
                })
                .unwrap();
            let labels = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .filter(|cell| cell.symbol() == "Q")
                .collect::<Vec<_>>();
            assert!(!labels.is_empty(), "media path or caption must appear");
            assert!(
                labels
                    .iter()
                    .all(|cell| cell.modifier.contains(Modifier::DIM))
            );
            assert!(
                media_plan.into_effects().is_empty(),
                "pending local media must not trigger a download"
            );
        }
    }

    #[test]
    fn status_media_is_collected_for_post_draw_dispatch() {
        let mut app = TestApp::new();
        let contact: wr::JID = "status@example.test".to_owned().into();
        let message_id: wr::MessageId = "status-media".into();
        app.open_status_contact = Some(contact.clone());
        app.messages.insert(
            message_id.clone(),
            wr::Message {
                info: wr::MessageInfo {
                    id: message_id.clone(),
                    chat: STATUS_BROADCAST_CHAT.to_owned().into(),
                    sender: contact,
                    mentions_self: false,
                    timestamp: 1,
                    is_from_me: false,
                    quote_id: None,
                    read_by: 0,
                    forwarding: Default::default(),
                },
                message: wr::MessageContent::File(wr::FileContent {
                    kind: wr::FileKind::Image,
                    path: "status.png".into(),
                    ..Default::default()
                }),
            },
        );
        app.chat_messages.insert(
            STATUS_BROADCAST_CHAT.to_owned().into(),
            vec![message_id.clone()],
        );
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        let mut media_render_plan = crate::app::events::MediaRenderPlan::default();
        let mut visibility_plan = VisibilityPlan::default();

        terminal
            .draw(|frame| {
                render_statuses_with_plan(
                    frame,
                    &mut app,
                    &mut media_render_plan,
                    &mut visibility_plan,
                    Rect::new(0, 0, 40, 12),
                )
            })
            .unwrap();

        assert!(matches!(
            media_render_plan.into_effects().as_slice(),
            [crate::app::events::MediaRenderEffect::DownloadFile(id, _)] if id == &message_id
        ));
    }
}
