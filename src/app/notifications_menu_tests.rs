use super::actions::{ActionNotice, FocusPane};
use super::chat_store::write_port::{
    ChatStoreWritePort, PersistChat, PersistChatMessage, PersistMessage,
};
use super::contextual_actions::RowStyle;
use super::test_support::{FakeChatSettingsQuery, FixedClock, RecordingNotifier, TestApp, message};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::sync::{Arc, Mutex};
use whatsrust as wr;

fn press(app: &mut TestApp, code: KeyCode) {
    app.on_terminal_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn characters(app: &mut TestApp, keys: &str) {
    for character in keys.chars() {
        press(app, KeyCode::Char(character));
    }
}

fn open_notifications(app: &mut TestApp, enabled: bool) {
    characters(app, " a");
    assert!(app.leader_menu.is_none());
    let rows = &app
        .contextual_menu
        .as_ref()
        .expect("Space+a opens actions")
        .0;
    let matches = rows
        .iter()
        .filter(|row| row.display_shortcut == 'n')
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "one global notification action");
    assert_eq!(
        matches[0].display_label,
        if enabled {
            "Notifications: On"
        } else {
            "Notifications: Off"
        }
    );
    assert_eq!(matches[0].row_style, RowStyle::Enabled);
    assert_eq!(matches[0].reason, None);
}

#[derive(Clone, Default)]
struct RecordingWrites(Arc<Mutex<Vec<wr::Message>>>);

impl ChatStoreWritePort for RecordingWrites {
    fn persist(&self, command: PersistChatMessage) {
        self.0.lock().unwrap().push(command.message);
    }

    fn persist_chat(&self, _: PersistChat) {}

    fn persist_message(&self, command: PersistMessage) {
        self.0.lock().unwrap().push(command.message);
    }
}

#[test]
fn notifications_menu_is_global_and_round_trips_selected_settings_without_losing_preferences() {
    let unrelated = "# keep this comment\nmouse=disable\ncomposer_direction=rtl\nnotification_sound=system\ncustom=keep\n";
    for focus in [
        FocusPane::SectionRail,
        FocusPane::ChatList,
        FocusPane::Conversation,
    ] {
        for initial in [None, Some(true), Some(false)] {
            let original = match initial {
                None => unrelated.to_owned(),
                Some(enabled) => format!(
                    "{unrelated}notifications={}\n",
                    if enabled { "enable" } else { "disable" }
                ),
            };
            let mut app = TestApp::with_settings_and_ports(
                Some(&original),
                FixedClock::new(1_000),
                RecordingNotifier::default(),
            );
            app.focus_pane = focus;
            app.composer.insert_text("unsent draft");
            let path = app.preferences_path.clone();
            let enabled = initial.unwrap_or(true);
            assert_eq!(app.notifications_enabled, enabled);
            for next in [!enabled, enabled] {
                open_notifications(&mut app, !next);
                characters(&mut app, "n");
                assert_eq!(app.notifications_enabled, next, "{focus:?} / {initial:?}");
                assert!(app.contextual_menu.is_none() && app.leader_menu.is_none());
                assert!(app.action_notice.is_none());
                assert!(!app.should_quit);
                assert_eq!(app.preferences_path, path);
                assert_eq!(
                    std::fs::read_to_string(&path).unwrap(),
                    format!(
                        "{unrelated}notifications={}\n",
                        if next { "enable" } else { "disable" }
                    )
                );
                assert!(!app.mouse_capture_enabled);
                assert_eq!(
                    app.composer_direction,
                    super::preferences::ComposerDirection::Rtl
                );
                assert_eq!(app.composer.text(), "unsent draft");
                assert_eq!(app.focus_pane, focus);
                open_notifications(&mut app, next);
                press(&mut app, KeyCode::Esc);
            }
            if initial.is_some() {
                assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            }
        }
    }
}

#[test]
fn notifications_menu_toggle_immediately_gates_delivery_but_not_ingestion_or_unread() {
    let notifier = RecordingNotifier::default();
    let settings = FakeChatSettingsQuery::default();
    let writes = RecordingWrites::default();
    let mut app = TestApp::with_settings_and_ports(
        Some("notifications=enable\n"),
        FixedClock::new(1_000),
        notifier.clone(),
    );
    app.install_chat_settings_query(Box::new(settings.clone()));
    app.set_chat_store_write(Box::new(writes.clone()));
    app.focus_pane = FocusPane::SectionRail;
    let chat = wr::JID::from("chat@g.us".to_owned());
    app.contacts.insert(chat.clone(), "Saved Group".into());
    assert!(app.handle_whatsapp_event(wr::Event::SyncProgress(42)));

    // This is the same ingestion boundary used by runtime AppInput::Message.
    assert!(app.process_message(message(&chat, "before", 999), false));
    open_notifications(&mut app, true);
    characters(&mut app, "n");
    assert!(app.process_message(message(&chat, "disabled", 1_000), false));
    assert_eq!(
        *notifier.notifications.lock().unwrap(),
        vec![("Saved Group".to_owned(), "before".to_owned())]
    );
    assert_eq!(*notifier.sound_requests.lock().unwrap(), vec![false]);
    assert_eq!(*settings.jids.lock().unwrap(), vec![chat.clone()]);
    assert_eq!(app.pending_new_messages(&chat), 2);
    open_notifications(&mut app, false);
    characters(&mut app, "n");
    assert!(app.process_message(message(&chat, "after", 1_001), false));
    assert!(!app.process_message(message(&chat, "history", 998), true));

    assert_eq!(
        *notifier.notifications.lock().unwrap(),
        vec![
            ("Saved Group".to_owned(), "before".to_owned()),
            ("Saved Group".to_owned(), "after".to_owned())
        ]
    );
    assert_eq!(*notifier.sound_requests.lock().unwrap(), vec![false, true]);
    assert_eq!(
        *settings.jids.lock().unwrap(),
        vec![chat.clone(), chat.clone()]
    );
    assert_eq!(app.pending_new_messages(&chat), 3);
    assert_eq!(app.chats[&chat].last_message_time, Some(1_001));
    assert_eq!(app.history_sync_percent, Some(42));
    let persisted = writes.0.lock().unwrap();
    assert_eq!(persisted.len(), 4);
    for (index, id) in ["before", "disabled", "after", "history"]
        .iter()
        .enumerate()
    {
        assert_eq!(persisted[index].info.id.as_ref(), *id);
        assert!(
            matches!(&app.messages[*id].message, wr::MessageContent::Text(text) if text.as_ref() == *id)
        );
    }
}

#[test]
fn notifications_menu_persistence_failure_keeps_state_and_reports_unavailable() {
    for enabled in [true, false] {
        let original = format!(
            "notifications={}\ncustom=keep\n",
            if enabled { "enable" } else { "disable" }
        );
        let notifier = RecordingNotifier::default();
        let mut app = TestApp::with_settings_and_ports(
            Some(&original),
            FixedClock::new(1_000),
            notifier.clone(),
        );
        let path = app.preferences_path.clone();
        // A directory target fails even under root; no permission-bit assumptions.
        let directory = tempfile::tempdir().unwrap();
        app.preferences_path = directory.path().to_path_buf();
        open_notifications(&mut app, enabled);
        characters(&mut app, "n");
        assert_eq!(app.notifications_enabled, enabled);
        assert!(
            matches!(&app.action_notice, Some(ActionNotice::Unavailable(text)) if text.starts_with("Could not persist notifications:"))
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert!(directory.path().is_dir());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        assert!(notifier.notifications.lock().unwrap().is_empty());
        assert!(notifier.sound_requests.lock().unwrap().is_empty());
        assert!(app.messages.is_empty());
        assert!(!app.should_quit);
        press(&mut app, KeyCode::Esc);
        open_notifications(&mut app, enabled);
    }
}

#[test]
fn notifications_menu_preserves_contextual_quit_and_modal_composer_precedence() {
    let original = "notifications=enable\n";
    let mut app = TestApp::with_settings_and_ports(
        Some(original),
        FixedClock::new(1_000),
        RecordingNotifier::default(),
    );
    app.focus_pane = FocusPane::Conversation;
    app.open_chat_by_jid("selected@s.whatsapp.net".to_owned().into());
    characters(&mut app, " n");
    assert!(
        app.notifications_enabled,
        "Space+n is not the contextual toggle"
    );
    assert!(app.contextual_menu.is_none());
    press(&mut app, KeyCode::Esc);
    characters(&mut app, "i an");
    assert_eq!(app.composer.text(), " an");
    assert!(app.leader_menu.is_none() && app.contextual_menu.is_none());
    press(&mut app, KeyCode::Esc);
    app.shortcut_popup = true;
    characters(&mut app, " an");
    assert!(app.shortcut_popup);
    assert!(app.leader_menu.is_none() && app.contextual_menu.is_none());
    press(&mut app, KeyCode::Esc);
    assert!(!app.shortcut_popup);
    assert!(app.notifications_enabled);
    assert_eq!(
        std::fs::read_to_string(&app.preferences_path).unwrap(),
        original
    );
    characters(&mut app, " aq");
    assert!(app.should_quit);
    assert!(app.leader_menu.is_none() && app.contextual_menu.is_none());
    assert!(app.notifications_enabled);
    assert_eq!(
        std::fs::read_to_string(&app.preferences_path).unwrap(),
        original
    );
}
