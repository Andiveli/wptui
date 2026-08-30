use super::status_list::{StatusList, StatusListItem};
use crate::app::actions::FocusPane;
use crate::app::{App, read_receipts::VisibilityPlan};
use crate::ui::message_list::render_status_messages_with_plan;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
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
    let title = app
        .open_status_contact()
        .map(|contact| app.contact_name(&contact).to_string())
        .unwrap_or_else(|| "Status".to_string());
    let border_color = if app.focus_pane == FocusPane::Conversation {
        Color::Green
    } else {
        Color::White
    };
    let block = Block::bordered()
        .title(title)
        .border_style(Style::default().fg(border_color));
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
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    use whatsrust as wr;

    use super::render_statuses_with_plan;
    use crate::app::read_receipts::VisibilityPlan;
    use crate::app::status_projection::STATUS_BROADCAST_CHAT;
    use crate::app::test_support::TestApp;

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
