use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::*;
use crate::app::actions::ConversationMode;
use crate::app::events::MediaRenderPlan;
use crate::app::read_receipts::VisibilityPlan;
use crate::app::test_support::{
    FakeDmResolver, FakeGroupInfoQuery, FakeGroupParticipantsQuery, TestApp,
};

fn press(app: &mut TestApp, code: KeyCode) {
    app.on_terminal_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn open(app: &mut TestApp) {
    for character in [' ', 'a', 'i'] {
        press(app, KeyCode::Char(character));
    }
}

fn screen(app: &mut TestApp, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut media = MediaRenderPlan::default();
    let mut visibility = VisibilityPlan::default();
    terminal
        .draw(|frame| {
            crate::ui::draw_with_plan(frame, app, &mut media, &mut visibility);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn app() -> TestApp {
    let mut app = TestApp::new();
    let contact = wr::JID::from("15551234567@s.whatsapp.net".to_owned());
    app.apply_contact_refresh(vec![(contact.clone(), "Saved contact".into())]);
    app.open_chat_by_jid(contact);
    app.focus_pane = FocusPane::Conversation;
    app
}

struct Query(HashMap<wr::JID, Result<Vec<wr::GroupParticipant>, String>>);

impl ContactInfoQuery for Query {
    fn participants(&self, group: &wr::JID) -> Result<Vec<wr::GroupParticipant>, String> {
        self.0
            .get(group)
            .expect("configure every group query")
            .clone()
    }
}

fn result(app: &mut TestApp) -> ContactInfoResult {
    let mut jobs = MediaJobOwner::new();
    app.schedule_contact_info(&mut jobs);
    let event = app.rx.recv_timeout(Duration::from_secs(2)).unwrap();
    jobs.shutdown();
    match event {
        AppInput::App(AppEvent::ContactInfoLoaded(result)) => result,
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn known_shared_groups_match_verified_phone_and_lid_not_names() {
    let lid = wr::JID::from("987654321@lid".to_owned());
    let phone = wr::JID::from("15551234567@s.whatsapp.net".to_owned());
    let resolver = FakeDmResolver::with_result(Some(phone.clone()));
    let mut app = TestApp::with_dm_resolver(Box::new(resolver));
    app.focus_pane = FocusPane::Conversation;
    let a = wr::JID::from("a@g.us".to_owned());
    let b = wr::JID::from("b@g.us".to_owned());
    let c = wr::JID::from("c@g.us".to_owned());
    app.apply_contact_refresh(vec![
        (phone.clone(), "Saved contact".into()),
        (a.clone(), "Shared by LID".into()),
        (b.clone(), "Shared by phone".into()),
        (c.clone(), "Not shared".into()),
    ]);
    let participant = |jid, phone_number| wr::GroupParticipant {
        jid,
        phone_number,
        name: "Same display name".into(),
    };
    app.contact_info_query = Arc::new(Query(HashMap::from([
        (a, Ok(vec![participant(lid.clone(), lid.clone())])),
        (
            b,
            Ok(vec![participant("other@lid".to_owned().into(), phone)]),
        ),
        (
            c,
            Ok(vec![participant(
                "different@lid".to_owned().into(),
                "12223334444@s.whatsapp.net".to_owned().into(),
            )]),
        ),
    ])));
    app.open_chat_by_jid(lid);
    open(&mut app);
    assert!(screen(&mut app, 180, 50).contains("Loading shared groups"));
    let loaded = result(&mut app);
    assert!(app.handle_avatar_event(AppEvent::ContactInfoLoaded(loaded)));
    let rendered = screen(&mut app, 180, 50);
    assert!(rendered.contains("Saved contact"));
    assert!(rendered.contains("+15551234567"));
    assert!(rendered.contains("Shared groups · 2"));
    assert!(rendered.contains("Shared by LID"));
    assert!(rendered.contains("Shared by phone"));
    // The unshared name may appear in the contact list, never in details.
    let details = rendered
        .lines()
        .map(|line| line.chars().skip(134).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!details.contains("Not shared"));
}

#[test]
fn composed_contact_query_forwards_ports_and_preserves_unavailable_results() {
    use std::sync::Mutex;

    for (info_fails, empty_roster) in [(false, false), (true, false), (false, true)] {
        let mut app = app();
        let group = wr::JID::from("team@g.us".to_owned());
        let contact = app.open_chat().unwrap();
        app.apply_contact_refresh(vec![(group.clone(), "Team".into())]);
        let trace = Arc::new(Mutex::new(Vec::new()));
        let info = Arc::new(FakeGroupInfoQuery {
            trace: Arc::clone(&trace),
            ..Default::default()
        });
        let participants = Arc::new(FakeGroupParticipantsQuery {
            trace: Arc::clone(&trace),
            ..Default::default()
        });
        info.results.lock().unwrap().push_back(if info_fails {
            Err(wr::GroupInfoError::RequestFailed)
        } else {
            Ok(wr::GroupInfo {
                jid: group.clone(),
                name: "Team".into(),
                is_announce: false,
                is_admin: false,
            })
        });
        if !info_fails {
            let roster = if empty_roster {
                Vec::new()
            } else {
                vec![wr::GroupParticipant {
                    jid: contact.clone(),
                    phone_number: contact,
                    name: "Contact".into(),
                }]
            };
            participants.results.lock().unwrap().push_back(roster);
        }
        app.contact_info_query = Arc::new(WhatsRustContactInfoQuery::new(
            info.clone(),
            participants.clone(),
        ));
        open(&mut app);
        let loaded = result(&mut app);
        let expected = if info_fails {
            Err("RequestFailed".into())
        } else if empty_roster {
            Err("Participant lookup unavailable".into())
        } else {
            Ok(vec![SharedGroup {
                jid: group.clone(),
                name: "Team".into(),
            }])
        };
        assert_eq!(loaded.groups, expected);
        assert_eq!(info.calls.lock().unwrap().as_slice(), &[group.clone()]);
        let expected_participant_calls = if info_fails { Vec::new() } else { vec![group] };
        assert_eq!(
            *participants.calls.lock().unwrap(),
            expected_participant_calls
        );
        let expected_trace = if info_fails {
            vec!["info"]
        } else {
            vec!["info", "participants"]
        };
        assert_eq!(*trace.lock().unwrap(), expected_trace);
        assert!(app.handle_avatar_event(AppEvent::ContactInfoLoaded(loaded)));
        let rendered = screen(&mut app, 180, 50);
        if info_fails || empty_roster {
            assert!(rendered.contains("Shared groups unavailable"));
            assert!(!rendered.contains("No known shared groups"));
        } else {
            assert!(rendered.contains("Shared groups · 1"));
        }
    }
}

#[test]
fn composed_contact_query_keeps_shared_matches_when_one_candidate_is_unavailable() {
    use std::sync::Mutex;

    for (info_fails, failed_index) in [
        (true, 0),
        (true, 1),
        (true, 2),
        (false, 0),
        (false, 1),
        (false, 2),
    ] {
        let case = format!("info_fails={info_fails}, failed_index={failed_index}");
        let mut app = app();
        let contact = app.open_chat().unwrap();
        let groups = [
            (wr::JID::from("a@g.us".to_owned()), "Group A"),
            (wr::JID::from("b@g.us".to_owned()), "Group B"),
            (wr::JID::from("c@g.us".to_owned()), "Group C"),
        ];
        app.apply_contact_refresh(
            groups
                .iter()
                .map(|(jid, name)| (jid.clone(), Arc::<str>::from(*name)))
                .collect(),
        );
        let trace = Arc::new(Mutex::new(Vec::new()));
        let info = Arc::new(FakeGroupInfoQuery {
            trace: Arc::clone(&trace),
            ..Default::default()
        });
        let participants = Arc::new(FakeGroupParticipantsQuery {
            trace: Arc::clone(&trace),
            ..Default::default()
        });
        let mut expected_participant_calls = Vec::new();
        let mut expected_trace = Vec::new();
        for (index, (jid, name)) in groups.iter().enumerate() {
            expected_trace.push("info");
            if index == failed_index && info_fails {
                info.results
                    .lock()
                    .unwrap()
                    .push_back(Err(wr::GroupInfoError::RequestFailed));
                continue;
            }
            info.results.lock().unwrap().push_back(Ok(wr::GroupInfo {
                jid: jid.clone(),
                name: (*name).into(),
                is_announce: false,
                is_admin: false,
            }));
            expected_trace.push("participants");
            expected_participant_calls.push(jid.clone());
            // An empty port roster is ambiguous, not a reproduction of Go filtering.
            let roster = if index == failed_index {
                Vec::new()
            } else {
                vec![wr::GroupParticipant {
                    jid: if index % 2 == 0 {
                        contact.clone()
                    } else {
                        wr::JID::from("987654321@lid".to_owned())
                    },
                    phone_number: contact.clone(),
                    name: "Contact".into(),
                }]
            };
            participants.results.lock().unwrap().push_back(roster);
        }
        app.contact_info_query = Arc::new(WhatsRustContactInfoQuery::new(
            info.clone(),
            participants.clone(),
        ));
        open(&mut app);
        let loaded = result(&mut app);
        assert!(app.handle_avatar_event(AppEvent::ContactInfoLoaded(loaded)));
        let details = screen(&mut app, 180, 50)
            .lines()
            .map(|line| line.chars().skip(134).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(details.contains("Shared groups · 2"), "{case}: {details}");
        for (index, (_, name)) in groups.iter().enumerate() {
            assert_eq!(
                details.contains(*name),
                index != failed_index,
                "{case}: {details}"
            );
        }
        assert!(
            details.contains("Some groups unavailable"),
            "{case}: {details}"
        );
        assert!(!details.contains("Shared groups unavailable"), "{case}");
        assert!(!details.contains("No known shared groups"), "{case}");
        let expected_info_calls = groups
            .iter()
            .map(|(jid, _)| jid.clone())
            .collect::<Vec<_>>();
        assert_eq!(*info.calls.lock().unwrap(), expected_info_calls, "{case}");
        assert_eq!(
            *participants.calls.lock().unwrap(),
            expected_participant_calls,
            "{case}"
        );
        assert_eq!(*trace.lock().unwrap(), expected_trace, "{case}");
    }
}

#[test]
fn unknown_lid_is_not_displayed_as_a_phone_number() {
    let mut app = app();
    app.open_chat_by_jid("987654321@lid".to_owned().into());
    open(&mut app);
    let rendered = screen(&mut app, 180, 50);
    assert!(rendered.contains("Unknown contact"));
    assert!(rendered.contains("Phone"));
    assert!(rendered.contains("Unknown"));
    assert!(!rendered.contains("987654321"));
}

#[test]
fn shared_group_failure_is_not_reported_as_an_empty_membership() {
    let mut app = app();
    let group = wr::JID::from("team@g.us".to_owned());
    app.apply_contact_refresh(vec![(group.clone(), "Team".into())]);
    app.contact_info_query = Arc::new(Query(HashMap::from([(group, Err("offline".into()))])));
    open(&mut app);
    let loaded = result(&mut app);
    app.handle_avatar_event(AppEvent::ContactInfoLoaded(loaded));
    let rendered = screen(&mut app, 180, 50);
    assert!(rendered.contains("Shared groups unavailable"));
    assert!(!rendered.contains("No known shared groups"));
}

#[test]
fn empty_known_group_catalog_has_an_explicit_empty_state() {
    let mut app = app();
    app.contact_info_query = Arc::new(Query(HashMap::new()));
    open(&mut app);
    let loaded = result(&mut app);
    app.handle_avatar_event(AppEvent::ContactInfoLoaded(loaded));
    let rendered = screen(&mut app, 180, 50);
    assert!(rendered.contains("Shared groups · 0"));
    assert!(rendered.contains("No known shared groups"));
}

#[test]
fn switching_contact_and_reopening_rejects_previous_query_results() {
    let mut app = app();
    open(&mut app);
    let stale = result(&mut app);
    app.open_chat_by_jid("16667778888@s.whatsapp.net".to_owned().into());
    assert!(!screen(&mut app, 180, 50).contains("Contact information"));
    open(&mut app);
    assert!(!app.handle_avatar_event(AppEvent::ContactInfoLoaded(stale)));
    assert!(screen(&mut app, 180, 50).contains("Loading shared groups"));
    let stale = result(&mut app);
    press(&mut app, KeyCode::Esc);
    open(&mut app);
    assert!(!app.handle_avatar_event(AppEvent::ContactInfoLoaded(stale)));
    assert!(screen(&mut app, 180, 50).contains("Loading shared groups"));
}

#[test]
fn closing_details_cancels_remaining_group_reads_and_result_publication() {
    use std::sync::{Mutex, atomic::AtomicUsize, mpsc};

    struct BlockingQuery {
        started: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
        calls: AtomicUsize,
    }
    impl ContactInfoQuery for BlockingQuery {
        fn participants(&self, _: &wr::JID) -> Result<Vec<wr::GroupParticipant>, String> {
            if self.calls.fetch_add(1, Ordering::AcqRel) == 0 {
                self.started.send(()).unwrap();
                self.release.lock().unwrap().recv().unwrap();
            }
            Ok(Vec::new())
        }
    }
    let mut app = app();
    app.apply_contact_refresh(vec![
        ("a@g.us".to_owned().into(), "A".into()),
        ("b@g.us".to_owned().into(), "B".into()),
    ]);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let query = Arc::new(BlockingQuery {
        started: started_tx,
        release: Mutex::new(release_rx),
        calls: AtomicUsize::new(0),
    });
    app.contact_info_query = query.clone();
    open(&mut app);
    let mut jobs = MediaJobOwner::new();
    app.schedule_contact_info(&mut jobs);
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    press(&mut app, KeyCode::Esc);
    release_tx.send(()).unwrap();
    jobs.shutdown();
    assert_eq!(query.calls.load(Ordering::Acquire), 1);
    assert!(app.rx.try_recv().is_err());
    assert!(!screen(&mut app, 180, 50).contains("Contact information"));
}

#[test]
fn narrow_focus_navigation_does_not_target_temporarily_hidden_panes() {
    let mut app = app();
    open(&mut app);
    screen(&mut app, 80, 24);
    for character in ['h', 'l', '1', '2'] {
        press(&mut app, KeyCode::Char(character));
        assert_eq!(app.focus_pane, FocusPane::Conversation);
    }
    press(&mut app, KeyCode::Esc);
    screen(&mut app, 80, 24);
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.focus_pane, FocusPane::ChatList);
}

#[test]
fn narrow_contact_layout_closes_without_losing_the_draft() {
    for (width, height) in [(180, 50), (140, 40), (80, 24), (40, 12), (1, 1)] {
        let mut app = app();
        app.composer.insert_text("Draft kept");
        open(&mut app);
        let opened = screen(&mut app, width, height);
        if width >= 40 {
            assert!(opened.contains("Contact information"), "{width}x{height}");
        }
        if width >= 140 {
            assert!(opened.contains("Sections") && opened.contains("Contacts"));
        } else if width == 80 {
            assert!(!opened.contains("Sections") && !opened.contains("Contacts"));
        }
        press(&mut app, KeyCode::Esc);
        let closed = screen(&mut app, width, height);
        assert!(!closed.contains("Contact information"));
        if width >= 80 {
            assert!(closed.contains("Sections") && closed.contains("Contacts"));
        }
        assert_eq!(app.composer.text(), "Draft kept");
    }
}

#[test]
fn details_scroll_through_shared_groups_without_changing_the_open_conversation() {
    let mut app = app();
    let contact = app.open_chat();
    let groups = (0..30)
        .map(|index| {
            (
                wr::JID::from(format!("group-{index}@g.us")),
                Arc::<str>::from(format!("Group {index:02}")),
            )
        })
        .collect::<Vec<_>>();
    app.apply_contact_refresh(groups.clone());
    app.contact_info_query = Arc::new(Query(
        groups
            .into_iter()
            .map(|(jid, _)| {
                (
                    jid,
                    Ok(vec![wr::GroupParticipant {
                        jid: contact.clone().unwrap(),
                        phone_number: contact.clone().unwrap(),
                        name: "Contact".into(),
                    }]),
                )
            })
            .collect(),
    ));
    open(&mut app);
    let loaded = result(&mut app);
    app.handle_avatar_event(AppEvent::ContactInfoLoaded(loaded));
    assert!(screen(&mut app, 180, 50).contains("Group 00"));
    for _ in 0..29 {
        press(&mut app, KeyCode::Char('j'));
    }
    let rendered = screen(&mut app, 180, 50);
    let details = rendered
        .lines()
        .map(|line| line.chars().skip(134).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(details.contains("Group 29"));
    assert!(!details.contains("Group 00"));
    assert_eq!(app.open_chat(), contact);
    press(&mut app, KeyCode::Char('k'));
    assert!(screen(&mut app, 180, 50).contains("Group 28"));
}

#[test]
fn closing_contact_details_cancels_an_interrupted_navigation_prefix() {
    let mut app = app();
    open(&mut app);
    press(&mut app, KeyCode::Char('g'));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('i'));
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(app.composer.text(), "x");
    assert!(!screen(&mut app, 180, 50).contains("Contact information"));
}

#[test]
fn contact_pane_does_not_steal_composer_or_reaction_picker_input() {
    let mut app = app();
    open(&mut app);
    press(&mut app, KeyCode::Char('i'));
    for character in [' ', 'a', 'i', 'j', 'k'] {
        press(&mut app, KeyCode::Char(character));
    }
    assert_eq!(app.composer.text(), " aijk");
    assert_eq!(app.conversation_mode, ConversationMode::ComposerEditing);
    press(&mut app, KeyCode::Esc);
    assert!(screen(&mut app, 180, 50).contains("Contact information"));
    app.reaction_picker = Some((vec!["one".into(), "two".into()], 0));
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.reaction_picker.as_ref().unwrap().1, 1);
    press(&mut app, KeyCode::Esc);
    assert!(app.reaction_picker.is_none());
    assert!(screen(&mut app, 180, 50).contains("Contact information"));
    press(&mut app, KeyCode::Esc);
    assert!(!screen(&mut app, 180, 50).contains("Contact information"));
}

#[test]
fn group_and_status_contexts_cannot_open_user_contact_details() {
    let mut app = app();
    app.open_chat_by_jid("team@g.us".to_owned().into());
    open(&mut app);
    assert!(!screen(&mut app, 180, 50).contains("Phone"));
    press(&mut app, KeyCode::Esc);
    app.selected_section = Section::Status;
    open(&mut app);
    assert!(!screen(&mut app, 180, 50).contains("Phone"));
}
