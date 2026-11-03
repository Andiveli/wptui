use super::App;
use crate::app::actions::{AppAction, Section};
use crate::app::contextual_actions::{
    AvailabilityFacts, ContextualAction, ContextualContext, ImplementationStatus,
    contextual_menu_rows, evaluate_availability,
};

impl App<'_> {
    /// Runtime opt-in; called before callbacks capture a clone of diagnostics.
    pub fn configure_read_sync_trace_from_env(&mut self) {
        self.message_action_diagnostics = self
            .message_action_diagnostics
            .clone()
            .with_read_sync_trace(
                std::env::var_os("WPTUI_READ_SYNC_TRACE")
                    .filter(|path| !path.is_empty())
                    .map(Into::into),
            );
    }

    pub(crate) fn trace_read_sync_snapshot(
        &self,
        chat: &whatsrust::JID,
        stage: super::message_action_diagnostics::ReadSyncStage,
    ) {
        self.message_action_diagnostics.trace_snapshot(
            stage,
            self.chat_is_unread(chat),
            self.chat_is_manually_unread(chat),
            self.pending_new_messages(chat),
        );
    }

    pub(crate) fn contextual_context(&self) -> ContextualContext {
        let selected = self.selected_message();
        ContextualContext {
            focus: self.focus_pane,
            section: self.selected_section,
            has_selected_message: selected.is_some(),
            selected_text: selected.is_some_and(|message| {
                matches!(message.message, whatsrust::MessageContent::Text(_))
            }),
            selected_message_is_informational: selected.is_some_and(|message| {
                matches!(
                    message.message,
                    whatsrust::MessageContent::ViewOnceUnavailable
                )
            }),
            has_reference: selected.is_some_and(|message| message.info.quote_id.is_some()),
            attach_blocked: self.composer_blocked() || self.selected_section == Section::Status,
        }
    }

    fn selected_read_toggle_chat(&mut self) -> Option<whatsrust::JID> {
        if self.focus_pane != crate::app::actions::FocusPane::ChatList
            || self.selected_section == Section::Status
        {
            return None;
        }
        self.get_selected_chat().filter(|chat| {
            !chat.0.is_empty() && !Self::is_status_chat(chat) && self.chats.contains_key(chat)
        })
    }

    pub fn open_contextual_actions(&mut self) {
        let mut rows = contextual_menu_rows(self.contextual_context());
        if let Some(row) = rows
            .iter_mut()
            .find(|row| row.action_token == ContextualAction::MarkRead)
        {
            let chat = self.selected_read_toggle_chat();
            if let Some(chat) = &chat {
                self.trace_read_sync_snapshot(
                    chat,
                    super::message_action_diagnostics::ReadSyncStage::Menu,
                );
            }
            row.display_label = if chat.as_ref().is_some_and(|chat| self.chat_is_unread(chat)) {
                "Mark as read"
            } else {
                "Mark as unread"
            };
            row.row_style = if chat.is_some() {
                crate::app::contextual_actions::RowStyle::Enabled
            } else {
                crate::app::contextual_actions::RowStyle::Disabled
            };
            row.reason = chat.is_none().then_some("chat required");
        }
        self.contextual_menu = Some((rows, 0));
    }

    pub fn move_contextual_menu(&mut self, delta: isize) {
        if let Some((rows, selected)) = &mut self.contextual_menu {
            *selected = selected
                .saturating_add_signed(delta)
                .min(rows.len().saturating_sub(1));
        }
    }

    pub fn activate_contextual_action(&mut self) {
        let Some((rows, selected)) = self.contextual_menu.as_ref() else {
            return;
        };
        let Some(row) = rows.get(*selected).copied() else {
            return;
        };
        if row.action_token == ContextualAction::MarkRead {
            if let Some(chat) = self.selected_read_toggle_chat() {
                self.contextual_menu = None;
                self.toggle_chat_read(&chat);
            }
            return;
        }
        let context = self.contextual_context();
        let available = evaluate_availability(
            crate::app::contextual_actions::CONTEXTUAL_ACTION_METADATA
                .iter()
                .find(|metadata| metadata.action == row.action_token)
                .map_or(ImplementationStatus::Planned, |metadata| {
                    metadata.implementation
                }),
            Some(row.action_token),
            AvailabilityFacts {
                contextual: Some(context),
                contextual_activatable: false,
            },
        );
        if !available.activatable() {
            return;
        }
        self.contextual_menu = None;
        self.dispatch_action(contextual_action_to_app_action(row.action_token));
    }
}

fn contextual_action_to_app_action(action: ContextualAction) -> AppAction {
    match action {
        ContextualAction::Copy => AppAction::CopyMessage,
        ContextualAction::React => AppAction::ReactMessage,
        ContextualAction::Reply => AppAction::ReplyMessage,
        ContextualAction::Share => AppAction::ShareMessage,
        ContextualAction::ReplyPrivately => AppAction::ReplyPrivately,
        ContextualAction::Open => AppAction::OpenMessage,
        ContextualAction::ViewAttachment => AppAction::ViewMessage,
        ContextualAction::GoToReference => AppAction::GoToReference,
        ContextualAction::DeleteForEveryone => AppAction::DeleteMessage,
        ContextualAction::Attach => AppAction::AttachFile,
        ContextualAction::CreateStatus => AppAction::StartStatusComposition,
        ContextualAction::Quit => AppAction::Quit,
        _ => AppAction::PlannedLeaderAction(
            crate::app::contextual_actions::CONTEXTUAL_ACTION_METADATA
                .iter()
                .find(|meta| meta.action == action)
                .map_or("Contextual action", |meta| meta.label),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::{ComposerAction, FocusPane, Section};
    use crate::app::contextual_actions::RowStyle;
    use crate::app::test_support::TestApp;

    fn press(app: &mut TestApp, character: char) {
        use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        app.on_terminal_event(Event::Key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::NONE,
        )));
    }

    fn open_read_menu(app: &mut TestApp, label: &str) {
        press(app, ' ');
        press(app, 'a');
        let rows = &app
            .contextual_menu
            .as_ref()
            .expect("Space+a opens actions")
            .0;
        let row = rows
            .iter()
            .find(|row| row.action_token == ContextualAction::MarkRead)
            .expect("chat actions include the read toggle");
        assert_eq!(row.display_shortcut, 'm');
        assert_eq!(row.display_label, label);
        assert_eq!(row.row_style, RowStyle::Enabled);
    }

    #[test]
    fn read_sync_trace_runtime_env_captures_actual_disconnected_worker_result() {
        const CHILD: &str = "WPTUI_READ_SYNC_TRACE_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let directory = tempfile::tempdir().unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg(
                    concat!(
                        module_path!(),
                        "::read_sync_trace_runtime_env_captures_actual_disconnected_worker_result"
                    )
                    .split_once("::")
                    .unwrap()
                    .1,
                )
                .env(CHILD, "1")
                .env("WPTUI_READ_SYNC_TRACE", directory.path().join("trace.txt"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            let report = std::fs::read_to_string(directory.path().join("trace.txt")).unwrap();
            assert_eq!(report.lines().count(), 5);
            assert!(
                report
                    .lines()
                    .any(|line| line == "stage=bridge status=1 category=unavailable")
            );
            assert!(
                report
                    .lines()
                    .any(|line| line == "stage=queue accepted=true read=false")
            );
            return;
        }
        // A separate process owns the callbacks and has never created a Go client.
        // Thus the real async bridge returns unavailable without a network send.
        let mut app = TestApp::new();
        app.configure_read_sync_trace_from_env();
        let (tx, rx) = std::sync::mpsc::channel();
        crate::runtime_callbacks::register(tx, app.message_action_diagnostics.clone());
        app.set_chat_read_sync(Box::new(
            crate::chat_read_sync::WhatsRustChatReadSync::default(),
        ));
        app.handle_whatsapp_event(whatsrust::Event::Chat {
            jid: whatsrust::JID::from("private-empty@s.whatsapp.net".to_owned()),
            last_message_time: 0,
        });
        app.focus_pane = FocusPane::ChatList;
        app.chat_list_state.select(Some(0));
        open_read_menu(&mut app, "Mark as unread");
        press(&mut app, 'm');
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        app.shutdown_read_sync_worker();
        assert!(app.action_notice.is_none());
    }

    // The injected path models WPTUI_READ_SYNC_TRACE without process-global env mutation.
    #[test]
    fn read_sync_trace_observes_menu_toggle_and_queue_without_changing_behavior() {
        use crate::app::message_action_diagnostics::MessageActionDiagnostics;
        use crate::app::test_support::{
            FakeChatReadCursorPort, RecordingChatReadSyncPort, message,
        };
        use crate::ui::contact_list::ContactListItem;

        for enabled in [false, true] {
            for accepted in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("read-sync.txt");
                let mut app = TestApp::new();
                app.message_action_diagnostics = MessageActionDiagnostics::new(false)
                    .with_read_sync_trace(enabled.then_some(path.clone()));
                let chat = whatsrust::JID::from("private-chat@s.whatsapp.net".to_owned());
                let mut latest = message(&chat, "private-message-id", 1700000042);
                latest.message = whatsrust::MessageContent::Text("private message text".into());
                app.add_message(latest);
                app.sort_chats();
                app.focus_pane = FocusPane::ChatList;
                app.chat_list_state.select(Some(0));
                let sync = RecordingChatReadSyncPort::default();
                *sync.schedule_result.lock().unwrap() = accepted;
                app.set_chat_read_sync(Box::new(sync.clone()));
                let cursor = FakeChatReadCursorPort::default();
                app.set_chat_read_cursor(Box::new(cursor.clone()));
                let before = ContactListItem::from_chat(&app, &chat);
                let boundary = app.unread_boundary(&chat);

                open_read_menu(&mut app, "Mark as read");
                press(&mut app, 'm');
                assert!(app.contextual_menu.is_none());
                assert_eq!(app.open_chat(), None);
                assert_eq!(sync.state_calls.lock().unwrap().len(), 1);
                assert_eq!(sync.state_calls.lock().unwrap()[0].read, true);
                assert_eq!(cursor.stored.lock().unwrap().len(), usize::from(accepted));
                if accepted {
                    assert!(!ContactListItem::from_chat(&app, &chat).unread);
                    assert_eq!(app.pending_new_messages(&chat), 0);
                    assert_eq!(app.unread_boundary(&chat), None);
                    assert!(app.action_notice.is_none());
                } else {
                    assert_eq!(ContactListItem::from_chat(&app, &chat), before);
                    assert_eq!(app.pending_new_messages(&chat), 1);
                    assert_eq!(app.unread_boundary(&chat), boundary);
                    assert!(matches!(&app.action_notice,
                        Some(crate::app::actions::ActionNotice::Unavailable(text))
                        if text == "Could not schedule chat read/unread sync"));
                }
                open_read_menu(
                    &mut app,
                    if accepted {
                        "Mark as unread"
                    } else {
                        "Mark as read"
                    },
                );
                if enabled {
                    let unread = !accepted;
                    assert_eq!(
                        std::fs::read_to_string(&path).unwrap(),
                        format!(
                            "stage=menu unread=true manual=false pending=1\n\
                         stage=toggle_before unread=true manual=false pending=1\n\
                         stage=queue accepted={accepted} read=true\n\
                         stage=toggle_after unread={unread} manual=false pending={}\n\
                         stage=menu unread={unread} manual=false pending={}\n",
                            usize::from(unread),
                            usize::from(unread),
                        )
                    );
                } else {
                    assert!(!path.exists(), "disabled tracing must not create a report");
                }
            }
        }
    }

    #[test]
    fn read_sync_trace_is_bounded_and_observes_manual_unread() {
        use crate::app::message_action_diagnostics::MessageActionDiagnostics;
        use crate::app::test_support::RecordingChatReadSyncPort;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("read-sync.txt");
        let mut app = TestApp::new();
        app.message_action_diagnostics =
            MessageActionDiagnostics::new(false).with_read_sync_trace(Some(path.clone()));
        let sync = RecordingChatReadSyncPort::default();
        app.set_chat_read_sync(Box::new(sync.clone()));
        app.handle_whatsapp_event(whatsrust::Event::Chat {
            jid: whatsrust::JID::from("private-empty@s.whatsapp.net".to_owned()),
            last_message_time: 0,
        });
        app.focus_pane = FocusPane::ChatList;
        app.chat_list_state.select(Some(0));
        open_read_menu(&mut app, "Mark as unread");
        press(&mut app, 'm');
        open_read_menu(&mut app, "Mark as read");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "stage=menu unread=false manual=false pending=0\n\
             stage=toggle_before unread=false manual=false pending=0\n\
             stage=queue accepted=true read=false\n\
             stage=toggle_after unread=true manual=true pending=0\n\
             stage=menu unread=true manual=true pending=0\n"
        );
        for _ in 0..120 {
            app.open_contextual_actions();
        }
        let report = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            report.lines().count(),
            100,
            "stop writing at the report cap"
        );
        assert!(report.len() < 16 * 1024);
        assert_eq!(sync.state_calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn read_sync_trace_preserves_existing_file_and_both_toggle_directions() {
        use crate::app::message_action_diagnostics::MessageActionDiagnostics;
        use crate::app::test_support::{
            ChatReadStateSyncCall, FakeChatReadCursorPort, RecordingChatReadSyncPort,
        };
        use crate::ui::contact_list::ContactListItem;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("existing.txt");
        let original: &[u8] = b"existing report\n\x00\xff";
        std::fs::write(&path, original).unwrap();
        let mut app = TestApp::new();
        app.message_action_diagnostics =
            MessageActionDiagnostics::new(false).with_read_sync_trace(Some(path.clone()));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let sync = RecordingChatReadSyncPort::default();
        app.set_chat_read_sync(Box::new(sync.clone()));
        let cursor = FakeChatReadCursorPort::default();
        app.set_chat_read_cursor(Box::new(cursor.clone()));
        let chat = whatsrust::JID::from("empty@s.whatsapp.net".to_owned());
        app.handle_whatsapp_event(whatsrust::Event::Chat {
            jid: chat.clone(),
            last_message_time: 0,
        });
        app.focus_pane = FocusPane::ChatList;
        app.chat_list_state.select(Some(0));
        for (label, unread) in [("Mark as unread", true), ("Mark as read", false)] {
            open_read_menu(&mut app, label);
            press(&mut app, 'm');
            assert!(app.contextual_menu.is_none());
            assert_eq!(ContactListItem::from_chat(&app, &chat).unread, unread);
            assert_eq!(app.pending_new_messages(&chat), 0);
            assert_eq!(app.unread_boundary(&chat), None);
            assert!(cursor.stored.lock().unwrap().is_empty());
            assert!(app.action_notice.is_none());
            assert_eq!(app.open_chat(), None);
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
        assert_eq!(
            sync.state_calls.lock().unwrap().as_slice(),
            &[false, true].map(|read| ChatReadStateSyncCall {
                chat: chat.clone(),
                message_id: None,
                read,
                timestamp: 0,
                from_me: false,
                participant: None,
            })
        );
    }

    #[test]
    fn read_sync_trace_io_failure_does_not_reject_an_existing_toggle() {
        use crate::app::message_action_diagnostics::MessageActionDiagnostics;
        use crate::app::test_support::RecordingChatReadSyncPort;

        let directory = tempfile::tempdir().unwrap();
        let mut app = TestApp::new();
        // A directory cannot be opened as a report file; no permissions assumptions.
        app.message_action_diagnostics = MessageActionDiagnostics::new(false)
            .with_read_sync_trace(Some(directory.path().to_path_buf()));
        let sync = RecordingChatReadSyncPort::default();
        app.set_chat_read_sync(Box::new(sync.clone()));
        app.handle_whatsapp_event(whatsrust::Event::Chat {
            jid: whatsrust::JID::from("private-empty@s.whatsapp.net".to_owned()),
            last_message_time: 0,
        });
        app.focus_pane = FocusPane::ChatList;
        app.chat_list_state.select(Some(0));
        open_read_menu(&mut app, "Mark as unread");
        press(&mut app, 'm');
        open_read_menu(&mut app, "Mark as read");
        assert_eq!(sync.state_calls.lock().unwrap().len(), 1);
        assert!(app.action_notice.is_none());
        assert_eq!(app.open_chat(), None);
    }

    #[test]
    fn contextual_read_toggle_targets_the_visible_selected_chat() {
        use crate::app::test_support::{
            ChatReadStateSyncCall, ChatReadSyncCall, FakeChatReadCursorPort,
            RecordingChatReadSyncPort, message,
        };
        use crate::app::{ChatFilter, ContactRow};
        use crate::ui::contact_list::ContactListItem;
        use whatsrust::JID;

        let mut app = TestApp::new();
        let hidden = JID::from("hidden@s.whatsapp.net".to_owned());
        let selected = JID::from("selected@s.whatsapp.net".to_owned());
        let sync = RecordingChatReadSyncPort::default();
        let cursor = FakeChatReadCursorPort::default();
        app.set_chat_read_sync(Box::new(sync.clone()));
        app.set_chat_read_cursor(Box::new(cursor.clone()));
        app.add_message(message(&hidden, "hidden-message", 50));
        app.add_message(message(&selected, "latest", 42));
        app.sort_chats();
        app.focus_pane = FocusPane::ChatList;
        app.selected_section = Section::Chats;
        app.contact_search.input = "selected".into();
        app.chat_list_state.select(Some(0));
        assert_eq!(app.get_selected_chat(), Some(selected.clone()));
        assert_eq!(app.open_chat(), None);

        open_read_menu(&mut app, "Mark as read");
        press(&mut app, 'm');

        assert!(app.contextual_menu.is_none());
        assert_eq!(app.open_chat(), None);
        assert_eq!(app.pending_new_messages(&selected), 0);
        assert_eq!(app.pending_new_messages(&hidden), 1);
        assert_eq!(app.pending_chat_activity(&selected), (0, false));
        assert_eq!(app.unread_boundary(&selected), None);
        assert_eq!(
            sync.calls.lock().unwrap().as_slice(),
            &[ChatReadSyncCall {
                chat: selected.clone(),
                message_id: "latest".into(),
                timestamp: 42,
                from_me: false,
                participant: None,
            }]
        );
        let stored_before_unread = cursor.stored.lock().unwrap().len();
        assert_eq!(stored_before_unread, 1);
        let row = ContactRow::Chat(app.visible_chat_rows()[0].clone());
        let before = ContactListItem::from_contact_row(&app, &row);
        assert!(!before.unread);

        open_read_menu(&mut app, "Mark as unread");
        press(&mut app, 'm');

        assert!(app.contextual_menu.is_none());
        assert_eq!(app.open_chat(), None);
        let after = ContactListItem::from_contact_row(&app, &row);
        assert!(after.unread, "manual unread must have a visible indicator");
        assert!(!after.attention);
        assert_eq!(app.cached_contact_view().1[0], after);
        assert_eq!(app.unread_filter_counts(), [2, 2, 0, 0]);
        assert_eq!(
            after.preview, before.preview,
            "do not invent unread messages"
        );
        assert_eq!(app.pending_new_messages(&selected), 0);
        assert_eq!(app.pending_chat_activity(&selected), (0, false));
        assert_eq!(app.unread_boundary(&selected), None);
        assert_eq!(cursor.stored.lock().unwrap().len(), stored_before_unread);
        app.chat_filter = ChatFilter::Unread;
        assert_eq!(app.visible_chat_rows()[0].target, selected);

        // Reading an explicitly unread chat must work even at the same cursor.
        open_read_menu(&mut app, "Mark as read");
        press(&mut app, 'm');
        assert!(app.contextual_menu.is_none());
        assert!(!ContactListItem::from_contact_row(&app, &row).unread);
        assert!(app.visible_chat_rows().is_empty());
        assert_eq!(app.pending_new_messages(&hidden), 1);
        assert_eq!(
            sync.state_calls.lock().unwrap().as_slice(),
            &[true, false, true].map(|read| ChatReadStateSyncCall {
                chat: selected.clone(),
                message_id: Some("latest".into()),
                read,
                timestamp: 42,
                from_me: false,
                participant: None,
            })
        );
        assert_eq!(app.unread_filter_counts(), [1, 1, 0, 0]);
    }

    #[test]
    fn contextual_read_toggle_supports_a_chat_without_messages() {
        use crate::app::test_support::{
            ChatReadStateSyncCall, FakeChatReadCursorPort, RecordingChatReadSyncPort,
        };
        use crate::ui::contact_list::ContactListItem;
        let mut app = TestApp::new();
        let chat = whatsrust::JID::from("empty@s.whatsapp.net".to_owned());
        let cursor = FakeChatReadCursorPort::default();
        app.set_chat_read_cursor(Box::new(cursor.clone()));
        let sync = RecordingChatReadSyncPort::default();
        app.set_chat_read_sync(Box::new(sync.clone()));
        app.handle_whatsapp_event(whatsrust::Event::Chat {
            jid: chat.clone(),
            last_message_time: 0,
        });
        app.focus_pane = FocusPane::ChatList;
        app.selected_section = Section::Chats;
        app.chat_list_state.select(Some(0));

        for (label, unread) in [("Mark as unread", true), ("Mark as read", false)] {
            open_read_menu(&mut app, label);
            press(&mut app, 'm');
            assert!(app.contextual_menu.is_none());
            let row = app.visible_contact_rows().remove(0);
            let item = ContactListItem::from_contact_row(&app, &row);
            assert_eq!(item.unread, unread);
            assert_eq!(item.preview, "");
            assert!(!item.attention);
            assert_eq!(app.pending_new_messages(&chat), 0);
            assert_eq!(app.unread_boundary(&chat), None);
            assert!(cursor.stored.lock().unwrap().is_empty());
            assert_eq!(app.cached_contact_view().1[0], item);
        }
        assert_eq!(
            sync.state_calls.lock().unwrap().as_slice(),
            &[false, true].map(|read| ChatReadStateSyncCall {
                chat: chat.clone(),
                message_id: None,
                read,
                timestamp: 0,
                from_me: false,
                participant: None,
            })
        );
    }

    #[test]
    fn contextual_read_toggle_requires_a_chat_and_is_absent_from_status() {
        use crate::app::test_support::{FakeChatReadCursorPort, RecordingChatReadSyncPort};
        for section in [Section::Chats, Section::Status] {
            let mut app = TestApp::new();
            let sync = RecordingChatReadSyncPort::default();
            let cursor = FakeChatReadCursorPort::default();
            app.set_chat_read_sync(Box::new(sync.clone()));
            app.set_chat_read_cursor(Box::new(cursor.clone()));
            app.focus_pane = FocusPane::ChatList;
            app.selected_section = section;
            press(&mut app, ' ');
            press(&mut app, 'a');
            let rows = &app.contextual_menu.as_ref().unwrap().0;
            let row = rows
                .iter()
                .find(|row| row.action_token == ContextualAction::MarkRead);
            if section == Section::Status {
                assert!(row.is_none());
            } else {
                assert_eq!(row.unwrap().row_style, RowStyle::Disabled);
            }
            press(&mut app, 'm');
            assert!(app.contextual_menu.is_some());
            assert!(sync.calls.lock().unwrap().is_empty());
            assert!(cursor.stored.lock().unwrap().is_empty());
            assert_eq!(app.open_chat(), None);
        }
    }

    #[test]
    fn rejected_contextual_read_toggle_keeps_local_state_and_reports_failure() {
        use crate::app::test_support::{
            FakeChatReadCursorPort, RecordingChatReadSyncPort, message,
        };
        use crate::ui::contact_list::ContactListItem;
        for initially_unread in [false, true] {
            let mut app = TestApp::new();
            let chat = whatsrust::JID::from("chat@s.whatsapp.net".to_owned());
            app.add_message(message(&chat, "latest", 42));
            app.sort_chats();
            if !initially_unread {
                app.mark_chat_read_at_latest(&chat);
            }
            let sync = RecordingChatReadSyncPort::default();
            *sync.schedule_result.lock().unwrap() = false;
            app.set_chat_read_sync(Box::new(sync.clone()));
            let cursor = FakeChatReadCursorPort::default();
            app.set_chat_read_cursor(Box::new(cursor.clone()));
            app.focus_pane = FocusPane::ChatList;
            app.chat_list_state.select(Some(0));
            let before = ContactListItem::from_chat(&app, &chat);
            let pending = app.pending_new_messages(&chat);
            let boundary = app.unread_boundary(&chat);
            open_read_menu(
                &mut app,
                if initially_unread {
                    "Mark as read"
                } else {
                    "Mark as unread"
                },
            );
            press(&mut app, 'm');
            assert_eq!(ContactListItem::from_chat(&app, &chat), before);
            assert_eq!(app.pending_new_messages(&chat), pending);
            assert_eq!(app.unread_boundary(&chat), boundary);
            assert!(cursor.stored.lock().unwrap().is_empty());
            assert!(matches!(&app.action_notice,
                Some(crate::app::actions::ActionNotice::Unavailable(text))
                if text == "Could not schedule chat read/unread sync"));
            let calls = sync.state_calls.lock().unwrap();
            assert_eq!(calls.len(), 1);
            assert_eq!(
                calls[0],
                crate::app::test_support::ChatReadStateSyncCall {
                    chat,
                    message_id: Some("latest".into()),
                    read: initially_unread,
                    timestamp: 42,
                    from_me: false,
                    participant: None,
                }
            );
        }
    }

    #[test]
    fn opening_manual_unread_preserves_old_anchor_and_actual_pending_messages() {
        use crate::app::test_support::{
            ChatReadStateSyncCall, FakeChatReadCursorPort, RecordingChatReadSyncPort, message,
        };
        use crate::ui::contact_list::ContactListItem;
        for direct in [false, true] {
            let mut app = TestApp::new();
            let chat = whatsrust::JID::from("chat@s.whatsapp.net".to_owned());
            app.add_message(message(&chat, "anchor", 10));
            app.mark_chat_read_at_latest(&chat);
            app.handle_whatsapp_event(whatsrust::Event::MarkChatAsRead {
                chat: chat.clone(),
                message_id: "anchor".into(),
                read: false,
                timestamp: 10,
                from_me: false,
                participant: None,
            });
            app.add_message(message(&chat, "later", 20));
            app.sort_chats();
            let sync = RecordingChatReadSyncPort::default();
            app.set_chat_read_sync(Box::new(sync.clone()));
            let cursor = FakeChatReadCursorPort::default();
            app.set_chat_read_cursor(Box::new(cursor.clone()));
            app.chat_list_state.select(Some(0));
            if direct {
                app.open_chat_by_jid(chat.clone());
            } else {
                app.open_selected_chat();
            }
            assert_eq!(app.open_chat(), Some(chat.clone()));
            assert_eq!(
                app.message_list_state.get_selected_message().as_deref(),
                Some("anchor")
            );
            assert_eq!(app.pending_new_messages(&chat), 1);
            assert_eq!(app.unread_boundary(&chat), Some((1, 20)));
            assert_eq!(app.read_receipts.pending_len(), 0);
            assert_eq!(app.read_receipts.in_flight_len(), 0);
            assert!(cursor.stored.lock().unwrap().is_empty());
            assert_eq!(
                sync.state_calls.lock().unwrap().as_slice(),
                &[ChatReadStateSyncCall {
                    chat: chat.clone(),
                    message_id: Some("anchor".into()),
                    read: true,
                    timestamp: 10,
                    from_me: false,
                    participant: None,
                }]
            );
            // A receipt for the later message clears real unread; the manual flag must be gone.
            app.handle_whatsapp_event(whatsrust::Event::Receipt {
                kind: whatsrust::ReceiptKind::ReadSelf,
                chat: chat.clone(),
                message_ids: vec!["later".into()],
            });
            assert!(!ContactListItem::from_chat(&app, &chat).unread);
            assert_eq!(app.pending_new_messages(&chat), 0);
            assert_eq!(sync.state_calls.lock().unwrap().len(), 1);
        }
    }

    #[test]
    fn opening_manual_read_changes_indicator_only_after_queue_acceptance() {
        use crate::app::test_support::{
            FakeChatReadCursorPort, RecordingChatReadSyncPort, message,
        };
        use crate::ui::contact_list::ContactListItem;
        for (direct, accepted) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut app = TestApp::new();
            let chat = whatsrust::JID::from("chat@s.whatsapp.net".to_owned());
            app.add_message(message(&chat, "anchor", 10));
            app.mark_chat_read_at_latest(&chat);
            app.handle_whatsapp_event(whatsrust::Event::MarkChatAsRead {
                chat: chat.clone(),
                message_id: "".into(),
                read: false,
                timestamp: 0,
                from_me: false,
                participant: None,
            });
            app.sort_chats();
            app.chat_list_state.select(Some(0));
            let sync = RecordingChatReadSyncPort::default();
            *sync.schedule_result.lock().unwrap() = accepted;
            app.set_chat_read_sync(Box::new(sync.clone()));
            let cursor = FakeChatReadCursorPort::default();
            app.set_chat_read_cursor(Box::new(cursor.clone()));
            let before = ContactListItem::from_chat(&app, &chat);
            if direct {
                app.open_chat_by_jid(chat.clone());
            } else {
                app.open_selected_chat();
            }
            let mut expected = before.clone();
            expected.unread = !accepted;
            assert_eq!(ContactListItem::from_chat(&app, &chat), expected);
            assert!(before.unread);
            assert_eq!(
                app.message_list_state.get_selected_message().as_deref(),
                Some("anchor")
            );
            assert_eq!(app.pending_new_messages(&chat), 0);
            assert_eq!(app.unread_boundary(&chat), None);
            assert!(cursor.stored.lock().unwrap().is_empty());
            assert_eq!(
                sync.state_calls.lock().unwrap().as_slice(),
                &[crate::app::test_support::ChatReadStateSyncCall {
                    chat,
                    message_id: Some("anchor".into()),
                    read: true,
                    timestamp: 10,
                    from_me: false,
                    participant: None,
                }]
            );
            if accepted {
                assert!(app.action_notice.is_none());
            } else {
                assert!(matches!(&app.action_notice,
                    Some(crate::app::actions::ActionNotice::Unavailable(text))
                    if text == "Could not schedule chat read/unread sync"));
            }
        }
    }

    #[test]
    fn contextual_group_toggle_keeps_latest_message_identity() {
        use crate::app::test_support::{ChatReadStateSyncCall, RecordingChatReadSyncPort, message};
        let mut app = TestApp::new();
        let chat = whatsrust::JID::from("team@g.us".to_owned());
        let sender = whatsrust::JID::from("alice@s.whatsapp.net".to_owned());
        let mut latest = message(&chat, "latest", 42);
        latest.info.sender = sender.clone();
        latest.info.is_from_me = true;
        app.add_message(latest);
        app.sort_chats();
        let sync = RecordingChatReadSyncPort::default();
        app.set_chat_read_sync(Box::new(sync.clone()));
        app.focus_pane = FocusPane::ChatList;
        app.chat_list_state.select(Some(0));
        open_read_menu(&mut app, "Mark as unread");
        press(&mut app, 'm');
        assert_eq!(app.unread_filter_counts(), [1, 1, 1, 0]);
        assert!(!crate::ui::contact_list::ContactListItem::from_chat(&app, &chat).attention);
        app.mark_chat_read_at_latest(&chat);
        assert_eq!(
            sync.state_calls.lock().unwrap().as_slice(),
            &[false, true].map(|read| ChatReadStateSyncCall {
                chat: chat.clone(),
                message_id: Some("latest".into()),
                read,
                timestamp: 42,
                from_me: true,
                participant: Some(sender.clone()),
            })
        );
        assert!(!crate::ui::contact_list::ContactListItem::from_chat(&app, &chat).unread);
    }

    #[test]
    fn contextual_attach_dispatches_the_existing_attach_action() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;
        app.selected_section = Section::Chats;
        app.open_contextual_actions();
        let attach = app
            .contextual_menu
            .as_ref()
            .unwrap()
            .0
            .iter()
            .position(|row| row.action_token == ContextualAction::Attach)
            .unwrap();
        assert_eq!(
            app.contextual_menu.as_ref().unwrap().0[attach].row_style,
            RowStyle::Enabled
        );
        app.contextual_menu.as_mut().unwrap().1 = attach;
        app.activate_contextual_action();
        assert!(app.file_picker.is_some());
    }

    #[test]
    fn contextual_quit_dispatches_through_the_existing_quit_path() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;
        app.selected_section = Section::Chats;
        app.open_contextual_actions();
        let quit = app
            .contextual_menu
            .as_ref()
            .unwrap()
            .0
            .iter()
            .position(|row| row.action_token == ContextualAction::Quit)
            .unwrap();
        app.contextual_menu.as_mut().unwrap().1 = quit;
        app.activate_contextual_action();
        assert!(app.should_quit);
        assert!(app.contextual_menu.is_none());
    }

    #[test]
    fn composer_direction_toggle_dispatches_through_app_action() {
        let mut app = TestApp::new();
        assert_eq!(
            app.composer_direction,
            crate::app::preferences::ComposerDirection::Auto
        );
        app.dispatch_action(AppAction::ToggleComposerDirection);
        assert_eq!(
            app.composer_direction,
            crate::app::preferences::ComposerDirection::Rtl
        );
        app.dispatch_action(AppAction::ToggleComposerDirection);
        assert_eq!(
            app.composer_direction,
            crate::app::preferences::ComposerDirection::Auto
        );
    }

    #[test]
    fn toggling_direction_preserves_the_existing_draft_and_cursor() {
        let mut app = TestApp::new();
        app.composer.insert_text("abc אבג");
        let text = app.composer.text();
        let cursor = app.composer.input.cursor();

        app.dispatch_action(AppAction::ToggleComposerDirection);

        assert_eq!(app.composer.text(), text);
        assert_eq!(app.composer.input.cursor(), cursor);
    }

    #[test]
    fn toggling_direction_preserves_mentions_quote_and_pending_attachments() {
        let mut app = TestApp::new();
        app.composer
            .set_group_participants(vec![whatsrust::GroupParticipant {
                jid: "111@s.whatsapp.net".to_owned().into(),
                phone_number: "111@s.whatsapp.net".to_owned().into(),
                name: "Alice".into(),
            }]);
        app.composer.insert_text("@al");
        app.composer.confirm_mention();
        app.composer.quote = Some(whatsrust::Message {
            info: whatsrust::MessageInfo {
                id: "quoted".into(),
                chat: "123@g.us".to_owned().into(),
                sender: "222@s.whatsapp.net".to_owned().into(),
                mentions_self: false,
                timestamp: 0,
                is_from_me: false,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: whatsrust::MessageContent::Text("quoted text".into()),
        });
        app.composer
            .queue_attachment("photo.jpg".into(), whatsrust::FileKind::Image);

        app.dispatch_action(AppAction::ToggleComposerDirection);

        assert_eq!(app.composer.text(), "@Alice ");
        assert!(
            app.composer
                .quote
                .as_ref()
                .is_some_and(|quote| quote.info.id.as_ref() == "quoted")
        );
        assert_eq!(app.composer.pending.len(), 1);
        assert_eq!(app.composer.pending[0].path.as_ref(), "photo.jpg");
        let outcome = app.composer.apply(ComposerAction::Submit);
        assert!(matches!(
            outcome,
            crate::app::composer::ComposerOutcome::Submit {
                quote: Some(quote),
                mentions,
                messages,
                ..
            } if quote.info.id.as_ref() == "quoted"
                && mentions == vec![whatsrust::Mention {
                    jid: "111@s.whatsapp.net".to_owned().into(),
                    numeric_user: "111".into(),
                }]
                && messages.iter().any(|message| matches!(
                    message,
                    whatsrust::MessageContent::File(file)
                        if file.path.as_ref() == "photo.jpg"
                            && file.caption.as_deref() == Some("@111 ")
                ))
        ));
    }

    #[test]
    fn direction_toggle_surfaces_persistence_failure_without_changing_state() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = TestApp::new();
        app.preferences_path = directory.path().to_path_buf();

        app.dispatch_action(AppAction::ToggleComposerDirection);

        assert_eq!(
            app.composer_direction,
            crate::app::preferences::ComposerDirection::Auto
        );
        assert!(matches!(
            &app.action_notice,
            Some(crate::app::actions::ActionNotice::Unavailable(message))
                if message.starts_with("Could not persist composer direction:")
        ));
    }

    #[test]
    fn contextual_attach_does_not_activate_when_composer_is_blocked() {
        let mut app = TestApp::new();
        app.focus_pane = FocusPane::Conversation;
        app.selected_section = Section::Chats;
        let group = whatsrust::JID("123@g.us".into());
        app.open_chat_by_jid(group.clone());
        app.group_permissions.insert(
            group.clone(),
            whatsrust::GroupInfo {
                jid: group,
                name: "Admins only".into(),
                is_announce: true,
                is_admin: false,
            },
        );
        app.open_contextual_actions();
        let attach = app
            .contextual_menu
            .as_ref()
            .unwrap()
            .0
            .iter()
            .position(|row| row.action_token == ContextualAction::Attach)
            .unwrap();
        assert_eq!(
            app.contextual_menu.as_ref().unwrap().0[attach].row_style,
            RowStyle::Disabled
        );
        app.contextual_menu.as_mut().unwrap().1 = attach;
        app.activate_contextual_action();
        assert!(app.file_picker.is_none());
        assert!(app.contextual_menu.is_some());
    }
}
