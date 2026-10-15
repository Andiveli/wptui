use super::App;
use crate::app::read_receipts::VisibilityPlan;
use crate::app::{
    actions::ComposerAction,
    test_support::{FakeGroupInfoQuery, FakeGroupParticipantsQuery, GroupQueryTrace, TestApp},
};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use whatsrust as wr;

#[test]
fn is_group_chat_detects_group_jids() {
    assert!(App::is_group_chat(&wr::JID::from("123@g.us".to_owned())));
    assert!(!App::is_group_chat(&wr::JID::from(
        "123@s.whatsapp.net".to_owned()
    )));
    assert!(!App::is_group_chat(&wr::JID::from(
        "status@broadcast".to_owned()
    )));
}

fn group_queries(
    app: &mut TestApp,
    infos: Vec<Result<wr::GroupInfo, wr::GroupInfoError>>,
    participants: Vec<Vec<wr::GroupParticipant>>,
) -> (
    FakeGroupInfoQuery,
    FakeGroupParticipantsQuery,
    GroupQueryTrace,
) {
    let trace = Arc::new(Mutex::new(Vec::new()));
    let info = FakeGroupInfoQuery {
        results: Arc::new(Mutex::new(VecDeque::from(infos))),
        calls: Default::default(),
        trace: trace.clone(),
    };
    let participants = FakeGroupParticipantsQuery {
        results: Arc::new(Mutex::new(VecDeque::from(participants))),
        calls: Default::default(),
        trace: trace.clone(),
    };
    app.set_group_info_query(Box::new(info.clone()));
    app.set_group_participants_query(Box::new(participants.clone()));
    (info, participants, trace)
}

fn info(group: &wr::JID, announce: bool, admin: bool) -> wr::GroupInfo {
    wr::GroupInfo {
        jid: group.clone(),
        name: "Group".into(),
        is_announce: announce,
        is_admin: admin,
    }
}
fn participant() -> wr::GroupParticipant {
    wr::GroupParticipant {
        jid: "111@s.whatsapp.net".to_owned().into(),
        phone_number: "111@s.whatsapp.net".to_owned().into(),
        name: "Alice".into(),
    }
}

#[test]
fn opening_a_non_group_skips_group_queries_and_clears_participants() {
    let mut app = TestApp::new();
    let (info, participants, _) = group_queries(&mut app, vec![], vec![]);
    app.open_chat_by_jid("alice@s.whatsapp.net".to_owned().into());
    app.composer.insert_text("@a");
    assert!(info.calls.lock().unwrap().is_empty() && participants.calls.lock().unwrap().is_empty());
    assert!(!app.composer.mention_picker_active());
}

#[test]
fn opening_a_group_queries_info_then_participants_and_applies_both() {
    let mut app = TestApp::new();
    let group = wr::JID::from("123@g.us".to_owned());
    let expected = info(&group, false, false);
    let (info, participants, trace) = group_queries(
        &mut app,
        vec![Ok(expected.clone())],
        vec![vec![participant()]],
    );
    app.open_chat_by_jid(group.clone());
    app.composer.insert_text("@a");
    app.composer.confirm_mention();
    assert_eq!(info.calls.lock().unwrap().as_slice(), &[group.clone()]);
    assert_eq!(
        participants.calls.lock().unwrap().as_slice(),
        &[group.clone()]
    );
    assert_eq!(*trace.lock().unwrap(), ["info", "participants"]);
    assert_eq!(app.group_permissions.get(&group), Some(&expected));
    assert!(!app.composer_blocked());
    assert!(
        matches!(app.composer.apply(ComposerAction::Submit), crate::app::composer::ComposerOutcome::Submit { mentions, .. } if mentions == vec![wr::Mention { jid: participant().jid, numeric_user: "111".into() }])
    );
}

#[test]
fn reopening_a_group_requeries_without_a_cache() {
    let mut app = TestApp::new();
    let group = wr::JID::from("123@g.us".to_owned());
    let (info, participants, trace) = group_queries(
        &mut app,
        vec![
            Ok(info(&group, false, false)),
            Ok(info(&group, false, false)),
        ],
        vec![vec![], vec![]],
    );
    app.open_chat_by_jid(group.clone());
    app.open_chat_by_jid(group.clone());
    assert_eq!(
        info.calls.lock().unwrap().as_slice(),
        &[group.clone(), group.clone()]
    );
    assert_eq!(
        participants.calls.lock().unwrap().as_slice(),
        &[group.clone(), group]
    );
    assert_eq!(
        *trace.lock().unwrap(),
        ["info", "participants", "info", "participants"]
    );
}

#[test]
fn failed_group_info_removes_stale_permission_but_still_refreshes_participants() {
    let mut app = TestApp::new();
    let group = wr::JID::from("123@g.us".to_owned());
    app.group_permissions
        .insert(group.clone(), info(&group, true, false));
    let (_, participants, _) = group_queries(
        &mut app,
        vec![Err(wr::GroupInfoError::RequestFailed)],
        vec![vec![participant()]],
    );
    app.open_chat_by_jid(group.clone());
    app.composer.insert_text("@a");
    assert!(!app.group_permissions.contains_key(&group) && !app.composer_blocked());
    assert_eq!(participants.calls.lock().unwrap().as_slice(), &[group]);
    assert!(app.composer.mention_picker_active());
}

#[test]
fn empty_participants_do_not_discard_group_permission() {
    let mut app = TestApp::new();
    let group = wr::JID::from("123@g.us".to_owned());
    let expected = info(&group, true, false);
    let _ = group_queries(&mut app, vec![Ok(expected.clone())], vec![vec![]]);
    app.open_chat_by_jid(group.clone());
    app.composer.insert_text("@a");
    assert_eq!(app.group_permissions.get(&group), Some(&expected));
    assert!(app.composer_blocked());
    assert!(!app.composer.mention_picker_active());
}

#[test]
fn open_chat_by_jid_registers_recipient_in_chat_list() {
    let mut app = TestApp::new();
    let recipient = wr::JID::from("alice@s.whatsapp.net".to_owned());

    app.open_chat_by_jid(recipient.clone());

    assert_eq!(app.open_chat(), Some(recipient.clone()));
    assert!(app.chats.contains_key(&recipient));
    assert!(app.sorted_chats.contains(&recipient));
}

#[test]
fn opening_chat_with_restored_cursor_renders_unread_messages() {
    let mut app = TestApp::new();
    let chat = wr::JID::from("alice@s.whatsapp.net".to_owned());

    app.add_message(crate::app::test_support::message(&chat, "read", 1));
    app.mark_chat_read_at_latest(&chat);
    app.add_message(crate::app::test_support::message(&chat, "unread", 2));
    app.open_chat_by_jid(chat);

    let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();
    let mut media_render_plan = crate::app::events::MediaRenderPlan::default();
    let mut visibility_plan = VisibilityPlan::default();
    terminal
        .draw(|frame| {
            crate::ui::draw_with_plan(
                frame,
                &mut app,
                &mut media_render_plan,
                &mut visibility_plan,
            );
        })
        .unwrap();
    assert_eq!(
        app.message_list_state.get_selected_message(),
        Some("read".into())
    );
}
