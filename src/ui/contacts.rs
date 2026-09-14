use super::contact_list::{AVATAR_HEIGHT, AVATAR_WIDTH, ContactList, visible_contact_rows};
use crate::app::App;
use crate::app::actions::FocusPane;
use crate::app::contact_avatars::AvatarTarget;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Style},
    widgets::{Block, Paragraph, StatefulWidget, Widget},
};
use ratatui_image::StatefulImage;

pub(crate) fn render_contacts(frame: &mut Frame, app: &mut App, area: Rect) {
    let (rows, items) = app.cached_contact_view();
    let mut list_area = area;
    if !app.contact_search.input.is_empty() || app.contact_search_active {
        let [search_area, new_list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Percentage(100)]).areas(area);
        list_area = new_list_area;

        let text = format!("/{}", app.contact_search.input);
        frame.render_widget(Paragraph::new(text), search_area);

        if app.contact_search_active {
            frame.set_cursor_position(Position::new(
                search_area.x + app.contact_search.character_index as u16 + 1,
                search_area.y,
            ));
        }
    }

    let block = Block::bordered()
        .title(if let Some(p) = app.history_sync_percent {
            format!("Contacts ({p}%)")
        } else {
            "Contacts".to_string()
        })
        .border_style(
            Style::default().fg(if app.focus_pane == FocusPane::ChatList {
                Color::Green
            } else {
                Color::White
            }),
        );
    let contacts_area = block.inner(list_area);
    block.render(list_area, frame.buffer_mut());
    frame.render_stateful_widget(
        ContactList::new(&items),
        contacts_area,
        &mut app.chat_list_state,
    );

    let visible = visible_contact_rows(&items, app.chat_list_state.offset(), contacts_area.height);
    for (index, relative_y) in visible {
        let y = contacts_area.y.saturating_add(relative_y);
        let avatar_area = Rect::new(
            contacts_area.x,
            y,
            AVATAR_WIDTH.min(contacts_area.width),
            AVATAR_HEIGHT.min(contacts_area.bottom().saturating_sub(y)),
        );
        // Partial Kitty placements can leave terminal artifacts after a scroll.
        if avatar_area.width == AVATAR_WIDTH
            && avatar_area.height == AVATAR_HEIGHT
            && let Some(target) = rows[index]
                .avatar_target()
                .map(|jid| AvatarTarget::Contact(jid.clone()))
            && let Some(protocol) = app.contact_avatars.protocol_mut(&target)
        {
            StatefulImage::default().render(avatar_area, frame.buffer_mut(), protocol);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Chat;
    use crate::app::runtime_diagnostics::{PerfClock, PerfEnvironment, RuntimeDiagnostics};
    use crate::app::test_support::TestApp;
    use ratatui::{Terminal, backend::TestBackend};
    use whatsrust as wr;

    struct EnabledEnvironment;

    impl PerfEnvironment for EnabledEnvironment {
        fn value(&self, name: &str) -> Option<String> {
            (name == "WPTUI_PERF").then(|| "1".into())
        }
    }

    struct FixedPerfClock;

    impl PerfClock for FixedPerfClock {
        fn now_us(&self) -> u64 {
            1
        }
    }

    #[test]
    fn stable_double_render_reuses_the_chat_view_cache() {
        let mut test_app = TestApp::new();
        let chat: wr::JID = "render@example.test".to_owned().into();
        test_app.chats.insert(
            chat.clone(),
            Chat {
                jid: chat.clone(),
                last_message_time: Some(1),
            },
        );
        test_app.sorted_chats = vec![chat];
        let cache_dir = tempfile::tempdir().unwrap();
        test_app.app.runtime_diagnostics = RuntimeDiagnostics::from_environment_with(
            &EnabledEnvironment,
            cache_dir.path(),
            || Box::new(FixedPerfClock),
        );
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_contacts(frame, &mut test_app.app, area)
            })
            .unwrap();
        assert_eq!(test_app.app.runtime_diagnostics.chat_view_counts(), (1, 0));

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_contacts(frame, &mut test_app.app, area)
            })
            .unwrap();
        assert_eq!(test_app.app.runtime_diagnostics.chat_view_counts(), (1, 1));
    }

    #[test]
    fn scrolling_updates_contact_geometry_without_scheduling_avatar_requests() {
        let mut test_app = TestApp::new();
        for index in 0..12 {
            let chat: wr::JID = format!("chat-{index:02}@example.test").into();
            test_app.chats.insert(
                chat.clone(),
                Chat {
                    jid: chat.clone(),
                    last_message_time: Some(0),
                },
            );
            test_app.sorted_chats.push(chat);
        }
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();

        terminal
            .draw(|frame| render_contacts(frame, &mut test_app.app, frame.area()))
            .unwrap();
        assert!(test_app.app.contact_avatars.requested_targets().is_empty());

        test_app.app.chat_list_state.select(Some(8));
        terminal
            .draw(|frame| render_contacts(frame, &mut test_app.app, frame.area()))
            .unwrap();

        assert_eq!(test_app.app.chat_list_state.selected(), Some(8));
        assert!(test_app.app.chat_list_state.offset() > 0);
        assert!(test_app.app.contact_avatars.requested_targets().is_empty());
    }
}
