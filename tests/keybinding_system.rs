use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use whatsrust::JID;
use wp_tui::app::actions::{
    AppAction, FocusPane, PaneVisibility, Section, SequenceResolution, focus_after,
    focus_after_visibility_change, resolve_sequence,
};
use wp_tui::key_handler::{Key, KeybindHandler};
use wp_tui::ui::message_list::MessageListState;
mod common;
use common::TestApp;

#[test]
fn keymap_resolves_navigation_and_focus_bindings() {
    let cases = [
        (vec![Key::c('j')], AppAction::SelectNext),
        (vec![Key::c('k')], AppAction::SelectPrevious),
        (vec![Key::c('g'), Key::c('g')], AppAction::JumpTop),
        (vec![Key::c('G')], AppAction::JumpBottom),
        (vec![Key::ctrl('d')], AppAction::HalfPageDown),
        (vec![Key::ctrl('u')], AppAction::HalfPageUp),
        (vec![Key::c('h')], AppAction::FocusPrevious),
        (vec![Key::c('l')], AppAction::FocusNext),
        (vec![Key::c('i')], AppAction::InsertMode),
        (vec![Key::c('y')], AppAction::CopyMessage),
        (vec![Key::c('r')], AppAction::ReactMessage),
        (vec![Key::c('R')], AppAction::ReplyMessage),
        (vec![Key::c('d')], AppAction::DeleteMessage),
        (vec![Key::c('e')], AppAction::EditMessage),
        (vec![Key::c('o')], AppAction::OpenMessage),
        (vec![Key::c('x')], AppAction::DownloadMessage),
        (vec![Key::c('v')], AppAction::ViewMessage),
        (vec![Key::k(KeyCode::Enter)], AppAction::OpenMessageMenu),
    ];

    for (keys, action) in cases {
        assert_eq!(
            resolve_sequence(&keys),
            SequenceResolution::Complete(action)
        );
    }
}

#[test]
fn keymap_reports_partial_and_cancelled_sequences() {
    assert_eq!(
        resolve_sequence(&[Key::c('g')]),
        SequenceResolution::Partial
    );
    assert_eq!(
        resolve_sequence(&[Key::c('g'), Key::c('x')]),
        SequenceResolution::Cancelled
    );
}

#[test]
fn keymap_resolves_section_visibility_sequences() {
    assert_eq!(
        resolve_sequence(&[Key::c(' ')]),
        SequenceResolution::Partial
    );
    assert_eq!(
        resolve_sequence(&[Key::c(' '), Key::c('1')]),
        SequenceResolution::Complete(AppAction::ToggleSectionRail)
    );
    assert_eq!(
        resolve_sequence(&[Key::c(' '), Key::c('2')]),
        SequenceResolution::Complete(AppAction::ToggleChatList)
    );
}

#[test]
fn shift_h_no_longer_binds_to_message_history() {
    // The message-history panel is removed; Shift+H must not resolve to a
    // history action (or any action at all).
    assert_eq!(
        resolve_sequence(&[Key::c('H')]),
        SequenceResolution::Cancelled
    );
}

#[test]
fn keymap_resolves_crossterm_ctrl_shift_l_representations() {
    for character in ['L', 'l'] {
        let event_key = Key {
            code: KeyCode::Char(character),
            modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        };

        assert_eq!(
            resolve_sequence(&[event_key]),
            SequenceResolution::Complete(AppAction::ToggleLogs)
        );
    }
    assert_eq!(
        resolve_sequence(&[Key::ctrl('l')]),
        SequenceResolution::Cancelled
    );
}

#[test]
fn focus_pane_cycles_through_visible_panes() {
    let all = PaneVisibility::default();
    assert_eq!(FocusPane::SectionRail.next(all), FocusPane::ChatList);
    assert_eq!(FocusPane::ChatList.next(all), FocusPane::Conversation);
    assert_eq!(FocusPane::Conversation.next(all), FocusPane::SectionRail);
    assert_eq!(
        FocusPane::SectionRail.previous(all),
        FocusPane::Conversation
    );

    let conversation_only = PaneVisibility {
        section_rail: false,
        chat_list: false,
    };
    assert_eq!(
        FocusPane::Conversation.next(conversation_only),
        FocusPane::Conversation
    );
    assert_eq!(Key::k(KeyCode::Esc).code, KeyCode::Esc);
}

#[test]
fn hidden_focused_panes_move_to_the_nearest_visible_pane() {
    assert_eq!(
        focus_after_visibility_change(
            FocusPane::SectionRail,
            PaneVisibility {
                section_rail: false,
                chat_list: true,
            },
        ),
        FocusPane::ChatList
    );
    assert_eq!(
        focus_after_visibility_change(
            FocusPane::ChatList,
            PaneVisibility {
                section_rail: true,
                chat_list: false,
            },
        ),
        FocusPane::SectionRail
    );
}

#[test]
fn sections_are_typed_and_cycle_without_chat_data() {
    assert_eq!(Section::default(), Section::Chats);
    assert_eq!(Section::Chats.next(), Section::Status);
    assert_eq!(Section::Status.next(), Section::Communities);
    assert_eq!(Section::Communities.previous(), Section::Status);
}

#[test]
fn toggles_rehome_focus_and_preserve_section_selection() {
    let mut app = TestApp::new();
    app.focus_pane = FocusPane::SectionRail;
    app.dispatch_action(AppAction::SelectNext);
    assert_eq!(app.selected_section, Section::Status);

    app.dispatch_action(AppAction::ToggleSectionRail);
    assert!(!app.pane_visibility.section_rail);
    assert_eq!(app.focus_pane, FocusPane::ChatList);

    app.dispatch_action(AppAction::ToggleChatList);
    assert!(!app.pane_visibility.chat_list);
    assert_eq!(app.focus_pane, FocusPane::Conversation);

    app.dispatch_action(AppAction::ToggleSectionRail);
    app.focus_pane = FocusPane::SectionRail;
    app.dispatch_action(AppAction::SelectPrevious);
    assert_eq!(app.selected_section, Section::Chats);
}

#[test]
fn handler_retains_partial_sequences_and_cancels_invalid_ones() {
    let mut handler = KeybindHandler::default();

    assert_eq!(handler.resolve(Key::c('g')), SequenceResolution::Partial);
    assert_eq!(handler.buffered_keys(), &[Key::c('g')]);
    assert_eq!(handler.resolve(Key::c('x')), SequenceResolution::Cancelled);
    assert!(handler.buffered_keys().is_empty());
}

#[test]
fn handler_returns_completed_action_and_clears_the_sequence() {
    let mut handler = KeybindHandler::default();

    assert_eq!(handler.resolve(Key::c('g')), SequenceResolution::Partial);
    assert_eq!(
        handler.resolve(Key::c('g')),
        SequenceResolution::Complete(AppAction::JumpTop)
    );
    assert!(handler.buffered_keys().is_empty());
}

#[test]
fn handler_escape_cancels_a_pending_sequence() {
    let mut handler = KeybindHandler::default();

    assert_eq!(handler.resolve(Key::c('g')), SequenceResolution::Partial);
    assert_eq!(
        handler.resolve(Key::k(KeyCode::Esc)),
        SequenceResolution::Cancelled
    );
    assert!(handler.buffered_keys().is_empty());
}

#[test]
fn navigation_actions_switch_only_the_requested_focus_pane() {
    let visibility = PaneVisibility::default();
    assert_eq!(
        focus_after(FocusPane::ChatList, &AppAction::FocusNext, visibility),
        FocusPane::Conversation
    );
    assert_eq!(
        focus_after(
            FocusPane::Conversation,
            &AppAction::FocusPrevious,
            visibility
        ),
        FocusPane::ChatList
    );
    assert_eq!(
        focus_after(FocusPane::Conversation, &AppAction::SelectNext, visibility),
        FocusPane::Conversation
    );
}

#[test]
fn message_navigation_stays_within_message_and_viewport_bounds() {
    let mut state = MessageListState::default();

    state.select_next_bounded(3);
    state.select_next_bounded(3);
    state.select_previous_bounded(3);
    assert_eq!(state.selected, Some(0));

    state.jump_bottom_bounded(3);
    assert_eq!(state.selected, Some(2));
    state.half_page_down_bounded(3, 2);
    assert_eq!(state.selected, Some(2));
    state.half_page_up_bounded(3, 2);
    assert_eq!(state.selected, Some(0));
}

#[test]
fn empty_message_navigation_keeps_selection_and_offset_clear() {
    let mut state = MessageListState::default();
    state.selected = Some(2);
    state.offset = 4;

    state.jump_top_bounded(0);
    state.half_page_down_bounded(0, 5);

    assert_eq!(state.selected, None);
    assert_eq!(state.offset, 0);
}

#[test]
fn open_chat_starts_none_on_default_app() {
    let app = TestApp::new();
    assert!(app.open_chat().is_none());
}

#[test]
fn chat_list_selection_moves_highlight_without_opening_chat() {
    let mut app = TestApp::new();
    let first = JID::from("first@example.test".to_owned());
    let second = JID::from("second@example.test".to_owned());
    app.sorted_chats.push(first.clone());
    app.sorted_chats.push(second);
    app.chat_list_state.select(Some(0));
    app.open_chat = Some(first.clone());
    app.focus_pane = FocusPane::ChatList;

    app.dispatch_action(AppAction::SelectNext);

    assert_eq!(app.chat_list_state.selected(), Some(1));
    assert_eq!(app.open_chat(), Some(first.clone()));

    app.dispatch_action(AppAction::SelectPrevious);

    assert_eq!(app.chat_list_state.selected(), Some(0));
    assert_eq!(app.open_chat(), Some(first));
}

#[test]
fn open_chat_action_opens_selection_and_focuses_conversation() {
    let mut app = TestApp::new();
    let chat = JID::from("chat@example.test".to_owned());
    app.sorted_chats.push(chat.clone());
    app.chat_list_state.select(Some(0));
    app.focus_pane = FocusPane::ChatList;
    app.message_list_state.selected = Some(3);
    app.message_list_state.offset = 2;

    app.dispatch_action(AppAction::OpenChat);

    assert_eq!(app.open_chat(), Some(chat));
    assert_eq!(app.focus_pane, FocusPane::Conversation);
    assert_eq!(app.message_list_state.selected, None);
    assert_eq!(app.message_list_state.offset, 0);
}

#[test]
fn open_selected_chat_is_noop_without_selection() {
    let mut app = TestApp::new();
    assert!(app.open_chat().is_none());
    app.open_selected_chat();
    assert!(app.open_chat().is_none());
    assert_eq!(app.focus_pane, FocusPane::ChatList);
}

#[test]
fn open_chat_selects_most_recent_message_when_chat_has_messages() {
    let mut app = TestApp::new();
    let chat = JID::from("chat@example.test".to_owned());
    app.sorted_chats.push(chat.clone());
    app.chat_messages
        .entry(chat.clone())
        .or_default()
        .push("m1".into());
    app.chat_list_state.select(Some(0));
    app.focus_pane = FocusPane::ChatList;

    app.dispatch_action(AppAction::OpenChat);

    assert_eq!(app.focus_pane, FocusPane::Conversation);
    assert_eq!(app.message_list_state.selected, Some(0));
}

#[test]
fn returning_to_conversation_preserves_message_selection() {
    let mut app = TestApp::new();
    let chat = JID::from("chat@example.test".to_owned());
    app.sorted_chats.push(chat.clone());
    app.chat_messages
        .entry(chat.clone())
        .or_default()
        .push("m1".into());
    app.chat_list_state.select(Some(0));
    app.open_chat = Some(chat);
    app.focus_pane = FocusPane::Conversation;
    app.message_list_state.selected = Some(5);

    app.dispatch_action(AppAction::FocusPrevious); // h: back to the chat list
    assert_eq!(app.focus_pane, FocusPane::ChatList);
    app.dispatch_action(AppAction::FocusNext); // l: back to the conversation
    assert_eq!(app.focus_pane, FocusPane::Conversation);
    assert_eq!(app.message_list_state.selected, Some(5));
}
