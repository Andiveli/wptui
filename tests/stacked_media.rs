use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use whatsrust::{FileContent, FileKind, JID, Message, MessageContent, MessageInfo};
use wp_tui::app::actions::{ConversationMode, FocusPane, Section};
use wp_tui::app::events::MediaRenderPlan;
use wp_tui::app::message_actions::{MessageAction, MessageActionKind};
use wp_tui::app::read_receipts::VisibilityPlan;
use wp_tui::ui::message_list::{render_messages_with_plan, render_status_messages_with_plan};

mod common;
use common::TestApp;

struct FixedClock;

impl wp_tui::app::Clock for FixedClock {
    fn unix_seconds(&self) -> Option<i64> {
        Some(1_700_000_010)
    }
}

fn media(id: &str, time: i64, sent: bool, video: bool) -> Message {
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: "stack@example.test".to_owned().into(),
            sender: "alice@example.test".to_owned().into(),
            timestamp: 1_700_000_000 + time,
            is_from_me: sent,
            mentions_self: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::File(FileContent {
            kind: if video {
                FileKind::Video
            } else {
                FileKind::Image
            },
            path: format!("{id}.png").into(),
            caption: Some(format!("caption-{id}").into()),
            ..Default::default()
        }),
    }
}

fn setup(messages: Vec<Message>) -> TestApp {
    let mut app = TestApp::new();
    app.clock = Box::new(FixedClock);
    let chat: JID = "stack@example.test".to_owned().into();
    // Populate a historical conversation without producing unread boundaries.
    for message in messages {
        app.chat_messages
            .entry(chat.clone())
            .or_default()
            .push(message.info.id.clone());
        app.messages.insert(message.info.id.clone(), message);
    }
    app.open_chat = Some(chat.clone());
    app.sorted_chats.push(chat);
    app.focus_pane = FocusPane::Conversation;
    app.conversation_mode = ConversationMode::MessageNavigation;
    app
}

fn render(app: &mut TestApp, width: u16, height: u16) -> (Vec<String>, VisibilityPlan) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut visibility = VisibilityPlan::default();
    let mut media = MediaRenderPlan::default();
    terminal
        .draw(|frame| {
            render_messages_with_plan(
                frame,
                app,
                &mut media,
                &mut visibility,
                Rect::new(0, 0, width, height),
            );
        })
        .unwrap();
    let rows = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect()
        })
        .collect();
    (rows, visibility)
}

fn key(app: &mut TestApp, character: char) {
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char(character),
        KeyModifiers::NONE,
    )));
}

#[test]
fn received_and_sent_mixed_media_paint_one_newest_front_with_a_pile_and_count() {
    for sent in [false, true] {
        let mut app = setup(vec![
            media("old", 0, sent, false),
            media("new", 1, sent, true),
        ]);
        let (rows, _) = render(&mut app, 60, 40);
        let painted = rows.join("\n");
        assert!(painted.contains("2 photos/videos"));
        assert!(
            !painted.contains("┌─"),
            "real previews replace the symbolic pile"
        );
        assert!(painted.contains("caption-new"));
        assert!(!painted.contains("caption-old"));
        assert_eq!(app.chat_messages.values().next().unwrap().len(), 2);
        assert_eq!(app.messages.len(), 2);
    }
}

#[test]
fn text_sender_and_direction_interrupt_stacks_without_hiding_ordinary_content() {
    for boundary in ["text", "sender", "direction", "document"] {
        let mut middle = media("middle", 1, false, true);
        match boundary {
            "text" => middle.message = MessageContent::Text("ordinary text".into()),
            "sender" => middle.info.sender = "bob@example.test".to_owned().into(),
            "direction" => middle.info.is_from_me = true,
            "document" => {
                if let MessageContent::File(file) = &mut middle.message {
                    file.kind = FileKind::Document;
                }
            }
            _ => unreachable!(),
        }
        let mut app = setup(vec![
            media("old", 0, false, false),
            middle,
            media("new", 2, false, true),
        ]);
        let (rows, _) = render(&mut app, 60, 60);
        let painted = rows.join("\n");
        assert!(!painted.contains("photos/videos"), "{boundary}");
        assert!(painted.contains("caption-old"));
        assert!(painted.contains("caption-new"));
        assert!(painted.contains(if boundary == "text" {
            "ordinary text"
        } else {
            "caption-middle"
        }));
    }
}

#[test]
fn keyboard_selects_every_original_and_viewer_keeps_the_whole_chat_pool() {
    let mut interruption = media("text", 2, false, false);
    interruption.message = MessageContent::Text("separator".into());
    let mut app = setup(vec![
        media("old", 0, false, false),
        media("new", 1, false, true),
        interruption,
        media("last", 3, false, false),
    ]);
    app.message_list_state.select(Some(2));
    let (rows, _) = render(&mut app, 60, 60);
    assert!(rows.join("\n").contains("caption-new"));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("new")
    );
    key(&mut app, 'k');
    let (rows, _) = render(&mut app, 60, 60);
    assert!(rows.join("\n").contains("caption-old"));
    assert!(!rows.join("\n").contains("caption-new"));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("old")
    );
    key(&mut app, 'v');
    let viewer = app.attachment_viewer.as_ref().unwrap();
    assert_eq!(viewer.message_id.as_ref(), "old");
    assert_eq!(viewer.index, 0);
    assert_eq!(viewer.attachment_count, 3);
    key(&mut app, 'l');
    assert_eq!(
        app.attachment_viewer.as_ref().unwrap().message_id.as_ref(),
        "new"
    );
    key(&mut app, 'l');
    assert_eq!(
        app.attachment_viewer.as_ref().unwrap().message_id.as_ref(),
        "last"
    );
    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Char('R'),
        KeyModifiers::SHIFT,
    )));
    assert_eq!(app.composer.quote.as_ref().unwrap().info.id.as_ref(), "old");
}

#[test]
fn clicking_the_front_caption_selects_the_actual_original_message() {
    let mut app = setup(vec![
        media("old", 0, false, false),
        media("new", 1, false, true),
    ]);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| {
            wp_tui::ui::draw_with_plan(
                frame,
                &mut app,
                &mut MediaRenderPlan::default(),
                &mut VisibilityPlan::default(),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let conversation = wp_tui::ui::navigation_areas(buffer.area, app.pane_visibility).conversation;
    // The sidebar preview also paints this caption; click only the message pane.
    let needle = "caption-new";
    let (x, y) = (conversation.top()..conversation.bottom())
        .flat_map(|y| {
            (conversation.left()..conversation.right().saturating_sub(needle.len() as u16))
                .map(move |x| (x, y))
        })
        .find(|&(x, y)| {
            (0..needle.len())
                .map(|dx| buffer[(x + dx as u16, y)].symbol())
                .collect::<String>()
                == needle
        })
        .expect("front caption is painted in the conversation pane");
    app.on_terminal_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("new")
    );
    render(&mut app, 60, 40);
    key(&mut app, 'v');
    assert_eq!(
        app.attachment_viewer.as_ref().unwrap().message_id.as_ref(),
        "new"
    );
}

#[test]
fn hidden_members_do_not_stage_read_receipts_when_the_front_is_visible() {
    let mut app = setup(vec![
        media("old", 0, false, false),
        media("new", 1, false, true),
    ]);
    let (_, visibility) = render(&mut app, 60, 40);
    app.apply_visibility_plan(visibility);
    let candidate = app
        .read_receipts
        .take_staged()
        .expect("visible front receipt");
    assert_eq!(candidate.message_id, "new");
    assert_eq!(app.read_receipts.take_staged(), None);
    app.message_list_state.set_selected_message("old".into());
    let (_, visibility) = render(&mut app, 60, 40);
    app.apply_visibility_plan(visibility);
    assert_eq!(app.read_receipts.take_staged().unwrap().message_id, "old");
    assert_eq!(app.read_receipts.take_staged(), None);
}

#[test]
fn changing_front_and_resizing_preserve_selection_and_recompute_scroll_geometry() {
    let mut app = setup(vec![
        media("old", 0, false, false),
        media("new", 1, false, true),
    ]);
    render(&mut app, 60, 40);
    let generation = app.message_height_cache.generation();
    app.message_list_state.set_selected_message("old".into());
    for width in [1, 4, 12, 60] {
        let (rows, _) = render(&mut app, width, 8);
        assert_eq!(rows.len(), 8);
        assert_eq!(
            app.message_list_state.get_selected_message().as_deref(),
            Some("old")
        );
    }
    assert!(app.message_height_cache.generation() > generation);
    app.message_list_state.reset();
    let (rows, _) = render(&mut app, 60, 40);
    assert!(rows.join("\n").contains("caption-new"));
    assert_eq!(app.message_list_state.offset, 0);
}

#[test]
fn status_media_keep_separate_cards_and_captions() {
    let mut app = setup(Vec::new());
    let sender: JID = "alice@example.test".to_owned().into();
    app.selected_section = Section::Status;
    app.open_status_contact = Some(sender);
    for (index, id) in ["old", "new"].into_iter().enumerate() {
        let mut message = media(id, index as i64, false, index == 1);
        message.info.chat = "status@broadcast".to_owned().into();
        app.add_message(message);
    }
    let mut terminal = Terminal::new(TestBackend::new(60, 40)).unwrap();
    terminal
        .draw(|frame| {
            render_status_messages_with_plan(
                frame,
                &mut app,
                &mut MediaRenderPlan::default(),
                &mut VisibilityPlan::default(),
                Rect::new(0, 0, 60, 40),
            );
        })
        .unwrap();
    let painted = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(painted.contains("caption-old"));
    assert!(painted.contains("caption-new"));
    assert!(!painted.contains("photos/videos"));
}

#[test]
fn unread_boundary_splits_media_and_keeps_the_divider_visible() {
    let mut app = setup(vec![media("old", 0, false, false)]);
    let chat = app.open_chat().unwrap();
    app.mark_chat_read_at_latest(&chat);
    app.open_chat = None;
    app.add_message(media("new", 1, false, true));
    app.add_message(media("last", 2, false, false));
    app.open_chat = Some(chat.clone());
    assert_eq!(app.unread_boundary(&chat).unwrap().0, 2);
    let (rows, _) = render(&mut app, 60, 60);
    let painted = rows.join("\n");
    assert!(painted.contains("2 photos/videos"));
    assert!(painted.contains("caption-old"));
    assert!(painted.contains("caption-last"));
    assert!(!painted.contains("caption-new"));
    assert!(painted.contains("unread"));
    assert_eq!(app.unread_boundary(&chat).unwrap().0, 2);
}

#[test]
fn deletion_reflows_an_existing_stack_and_preserves_the_selected_identity() {
    let mut app = setup(vec![
        media("old", 0, false, false),
        media("middle", 1, false, true),
        media("new", 2, false, false),
    ]);
    let (rows, _) = render(&mut app, 60, 60);
    assert!(rows.join("\n").contains("3 photos/videos"));
    app.message_list_state.set_selected_message("middle".into());
    let message = app.messages["middle"].clone();
    app.apply_message_action(MessageAction {
        action_id: "delete-middle".into(),
        target_message_id: message.info.id,
        chat: message.info.chat,
        sender: message.info.sender,
        kind: MessageActionKind::Delete,
        occurred_at: 1_700_000_005,
        arrival_order: 1,
    });
    let (rows, _) = render(&mut app, 60, 60);
    let painted = rows.join("\n");
    assert!(!painted.contains("photos/videos"));
    assert!(painted.contains("This message was deleted."));
    assert!(painted.contains("caption-old"));
    assert!(painted.contains("caption-new"));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("middle")
    );
}

#[test]
fn date_changes_split_stacks_but_elapsed_time_alone_adds_no_new_window() {
    use chrono::TimeZone;

    let noon = chrono::Local
        .with_ymd_and_hms(2024, 1, 2, 12, 0, 0)
        .single()
        .unwrap()
        .timestamp();
    for (gap, grouped) in [(600, true), (86_400, false)] {
        let mut old = media("old", 0, false, false);
        old.info.timestamp = noon;
        let mut new = media("new", 0, false, true);
        new.info.timestamp = noon + gap;
        let mut app = setup(vec![old, new]);
        let (rows, _) = render(&mut app, 60, 40);
        let painted = rows.join("\n");
        assert_eq!(painted.contains("2 photos/videos"), grouped);
        assert_eq!(painted.contains("caption-old"), !grouped);
    }
}

#[test]
fn selected_member_keeps_its_own_quote_and_caption() {
    let mut quote = media("quote", 0, false, false);
    quote.message = MessageContent::Text("quoted text".into());
    let mut old = media("old", 1, false, false);
    old.info.quote_id = Some("quote".into());
    let mut app = setup(vec![quote, old, media("new", 2, false, true)]);
    app.message_list_state.set_selected_message("old".into());
    let (rows, _) = render(&mut app, 60, 60);
    let painted = rows.join("\n");
    assert!(painted.contains("quoted text"));
    assert!(painted.contains("caption-old"));
    assert!(!painted.contains("caption-new"));
    assert_eq!(
        app.selected_message().unwrap().info.quote_id.as_deref(),
        Some("quote")
    );
}

#[test]
fn clamped_last_selection_brings_the_oldest_member_to_the_front() {
    let mut app = setup(vec![
        media("old", 0, false, false),
        media("new", 1, false, true),
    ]);
    app.message_list_state.select_last();
    let (rows, _) = render(&mut app, 60, 40);
    assert!(rows.join("\n").contains("caption-old"));
    assert!(!rows.join("\n").contains("caption-new"));
    assert_eq!(app.message_list_state.selected, Some(1));
}

#[test]
fn text_search_between_stacks_still_selects_original_text_indexes() {
    let mut text = media("text", 1, false, false);
    text.message = MessageContent::Text("find this text".into());
    let mut app = setup(vec![
        media("old", 0, false, false),
        text,
        media("new", 2, false, true),
        media("last", 3, false, false),
    ]);
    key(&mut app, '/');
    for character in "find".chars() {
        key(&mut app, character);
    }
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));
    let (rows, _) = render(&mut app, 60, 60);
    assert!(rows.join("\n").contains("find this text"));
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("text")
    );
    assert_eq!(app.message_list_state.selected, Some(2));
}
