use std::sync::Arc;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    widgets::Block,
};

use crate::app::App;
use crate::app::actions::Section;
use crate::app::contact_avatars::{AvatarTarget, prioritized_avatar_requests};
use crate::app::events::AppEvent;
use crate::app::runtime_diagnostics::Phase;
use crate::ui::communities::community_avatar_targets;
use crate::ui::contact_list::visible_contact_rows;
use crate::ui::navigation_areas;

fn contact_avatar_targets(app: &mut App, area: Rect) -> Vec<AvatarTarget> {
    let (rows, items) = app.cached_contact_view();
    let list_area = if app.contact_search.input.is_empty() && !app.contact_search_active {
        area
    } else {
        let [_, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Percentage(100)]).areas(area);
        list_area
    };
    let contacts_area = Block::bordered().inner(list_area);
    let visible = visible_contact_rows(&items, app.chat_list_state.offset(), contacts_area.height);
    let row_targets = rows
        .iter()
        .map(|row| row.avatar_target().cloned().map(AvatarTarget::Contact))
        .collect::<Vec<_>>();
    let target_positions = row_targets
        .iter()
        .scan(0usize, |position, target| {
            Some(target.as_ref().map(|_| {
                let current = *position;
                *position += 1;
                current
            }))
        })
        .collect::<Vec<_>>();
    let visible_positions = visible
        .iter()
        .filter_map(|(index, _)| target_positions[*index])
        .collect::<Vec<_>>();
    let offset = visible_positions.first().copied().unwrap_or_default();
    let visible_count = visible_positions
        .last()
        .map_or(0, |last| (*last).saturating_sub(offset).saturating_add(1));
    let selected = app
        .chat_list_state
        .selected()
        .and_then(|index| target_positions.get(index).copied().flatten());
    let targets = row_targets.into_iter().flatten().collect::<Vec<_>>();
    prioritized_avatar_requests(&targets, selected, offset, visible_count)
}

impl App<'_> {
    pub(crate) fn schedule_avatar_viewport(&mut self, terminal_area: Rect) {
        let content_area = if self.show_logs {
            let [content_area, _] =
                Layout::horizontal([Constraint::Percentage(67), Constraint::Percentage(33)])
                    .areas(terminal_area);
            content_area
        } else {
            terminal_area
        };
        let targets = if self.rail_on_logout || self.selected_section == Section::Status {
            Vec::new()
        } else if let Some(area) = navigation_areas(content_area, self.pane_visibility).chat_list {
            match self.selected_section {
                Section::Chats => contact_avatar_targets(self, area),
                Section::Communities if self.community_detail.is_some() => {
                    contact_avatar_targets(self, area)
                }
                Section::Communities => {
                    let targets = community_avatar_targets(&self.community_navigation_rows());
                    prioritized_avatar_requests(&targets, None, 0, targets.len())
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        let started = self.runtime_diagnostics.phase_started();
        self.contact_avatars
            .schedule(targets, self.tx.clone(), Arc::clone(&self.picker));
        if let Some(started) = started {
            self.runtime_diagnostics
                .record_phase_finished(Phase::AvatarScheduling, started);
        }
    }
}

impl App<'_> {
    pub(crate) fn shutdown_avatar_runtime(&mut self) {
        self.contact_avatars.shutdown();
    }

    pub(crate) fn handle_avatar_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::ContactAvatar(result) => self.contact_avatars.apply(result),
            AppEvent::ContactAvatarRefreshed { generation, target } => {
                self.contact_avatars.mark_refreshed(generation, target)
            }
            _ => unreachable!("runtime_loop must route only Avatar events to handle_avatar_event"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::Chat;
    use crate::app::contact_avatars::{AvatarResult, AvatarTarget};
    use crate::app::events::{AppEvent, MediaRenderPlan};
    use crate::app::test_support::TestApp;
    use crate::ui;
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    use whatsrust as wr;

    fn target() -> AvatarTarget {
        AvatarTarget::Contact(wr::JID::from("avatar@example.test".to_owned()))
    }

    #[test]
    fn post_draw_scheduling_tracks_the_scrolled_avatar_viewport() {
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
        let mut media_render_plan = MediaRenderPlan::default();

        terminal
            .draw(|frame| ui::draw_with_plan(frame, &mut test_app.app, &mut media_render_plan))
            .unwrap();
        test_app
            .app
            .schedule_avatar_viewport(Rect::new(0, 0, 80, 10));
        let initial = test_app.app.contact_avatars.requested_targets().to_vec();
        assert!(initial.iter().any(
            |target| target == &AvatarTarget::Contact("chat-00@example.test".to_owned().into())
        ));

        test_app.app.chat_list_state.select(Some(8));
        terminal
            .draw(|frame| ui::draw_with_plan(frame, &mut test_app.app, &mut media_render_plan))
            .unwrap();
        test_app
            .app
            .schedule_avatar_viewport(Rect::new(0, 0, 80, 10));
        let scrolled = test_app.app.contact_avatars.requested_targets();

        assert_ne!(scrolled, initial.as_slice());
        assert!(scrolled.iter().any(
            |target| target == &AvatarTarget::Contact("chat-08@example.test".to_owned().into())
        ));
        assert!(scrolled.iter().any(
            |target| target == &AvatarTarget::Contact("chat-11@example.test".to_owned().into())
        ));
        assert!(!scrolled.iter().any(
            |target| target == &AvatarTarget::Contact("chat-00@example.test".to_owned().into())
        ));
    }

    #[test]
    fn contact_avatar_result_preserves_apply_redraw_boolean() {
        let mut app = TestApp::new();

        assert!(
            !app.handle_avatar_event(AppEvent::ContactAvatar(AvatarResult::Failed {
                generation: 0,
                target: target(),
            }))
        );
    }

    #[test]
    fn contact_avatar_refresh_preserves_mark_refreshed_redraw_boolean() {
        let mut app = TestApp::new();

        assert!(!app.handle_avatar_event(AppEvent::ContactAvatarRefreshed {
            generation: 0,
            target: target(),
        }));
    }
}
