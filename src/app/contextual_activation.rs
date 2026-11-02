use super::App;
use crate::app::actions::{AppAction, Section};
use crate::app::contextual_actions::{
    AvailabilityFacts, ContextualAction, ContextualContext, ImplementationStatus,
    contextual_menu_rows, evaluate_availability,
};

impl App<'_> {
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

    pub fn open_contextual_actions(&mut self) {
        self.contextual_menu = Some((contextual_menu_rows(self.contextual_context()), 0));
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
    fn contextual_read_toggle_targets_the_visible_selected_chat() {
        use crate::app::test_support::{
            ChatReadSyncCall, FakeChatReadCursorPort, RecordingChatReadSyncPort, message,
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
    }

    #[test]
    fn contextual_read_toggle_supports_a_chat_without_messages() {
        use crate::app::test_support::FakeChatReadCursorPort;
        use crate::ui::contact_list::ContactListItem;
        let mut app = TestApp::new();
        let chat = whatsrust::JID::from("empty@s.whatsapp.net".to_owned());
        let cursor = FakeChatReadCursorPort::default();
        app.set_chat_read_cursor(Box::new(cursor.clone()));
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
        }
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
