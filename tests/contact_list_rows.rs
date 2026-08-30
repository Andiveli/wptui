use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
    widgets::{ListState, StatefulWidget},
};
use whatsrust::{JID, Message, MessageContent, MessageInfo};
use wp_tui::app::contact_avatars::prioritized_avatar_requests;
use wp_tui::app::{
    Chat, CommunityNode,
    actions::{PaneVisibility, Section},
    events::MediaRenderPlan,
};
use wp_tui::ui::{
    self,
    contact_list::{
        ContactList, ContactListItem, contact_viewport, contact_visible_range, format_row, initials,
    },
};
mod common;
use common::TestApp;

const UI_SOURCE: &str = include_str!("../src/ui.rs");
const CONTACTS_SOURCE: &str = include_str!("../src/ui/contacts.rs");
const COMMUNITIES_SOURCE: &str = include_str!("../src/ui/communities.rs");
const AVATAR_RUNTIME_SOURCE: &str = include_str!("../src/app/runtime_avatar_events.rs");

#[test]
fn initials_are_deterministic_for_names_and_unicode() {
    assert_eq!(initials(""), "");
    assert_eq!(initials("   "), "");
    assert_eq!(initials("alice"), "A");
    assert_eq!(initials("Alice Bob Carroll"), "AC");
    assert_eq!(initials("élise 李"), "É李");
}

#[test]
fn rows_reserve_right_content_and_truncate_deterministically() {
    assert_eq!(format_row("Alice", Some("12:34"), 12), "Alice  12:34");
    assert_eq!(format_row("Long contact", Some("12:34"), 8), "Lo 12:34");
    assert_eq!(format_row("李小龍", None, 4), "李小");
    assert_eq!(format_row("anything", Some("9"), 0), "");
}

#[test]
fn viewport_uses_three_rows_per_contact_and_keeps_selection_visible() {
    assert_eq!(contact_viewport(0, 0, 0, 5), (0, 0));
    assert_eq!(contact_viewport(3, 0, 6, 5), (3, 2));
    assert_eq!(contact_viewport(1, 3, 4, 5), (1, 1));
    assert_eq!(contact_viewport(4, 0, 7, 5), (4, 3));
}

#[test]
fn partial_contact_rows_are_clipped_and_reported_visible_without_avatar_overflow() {
    assert_eq!(contact_visible_range(1, 3, 5), 1..2);
    let items = vec![item("Alice", "one"), item("Bob", "two")];
    let mut state = ListState::default().with_selected(Some(1));
    let area = Rect::new(0, 0, 16, 3);
    let mut buffer = Buffer::empty(area);

    ContactList::new(&items).render(area, &mut buffer, &mut state);

    // Only Bob fits in 3 rows; his initials appear at row 0
    assert_eq!(buffer[(5, 0)].symbol(), "B");
}

#[test]
fn selection_highlights_both_rows_as_one_item() {
    let items = vec![item("Alice", "hello"), item("Bob", "goodbye")];
    let mut state = ListState::default().with_selected(Some(1));
    let area = Rect::new(0, 0, 16, 6);
    let mut buffer = Buffer::empty(area);

    ContactList::new(&items).render(area, &mut buffer, &mut state);

    // Alice (rows 0-2, not selected)
    assert_eq!(buffer[(0, 0)].bg, Color::Reset);
    assert_eq!(buffer[(0, 1)].bg, Color::Reset);
    assert_eq!(buffer[(0, 2)].bg, Color::Reset);
    // Bob (rows 3-5, selected)
    assert_eq!(buffer[(0, 3)].bg, Color::DarkGray);
    assert_eq!(buffer[(0, 4)].bg, Color::DarkGray);
    assert_eq!(buffer[(0, 5)].bg, Color::Reset);
    // Bob's initials at row 3, preview "g" at row 4
    assert_eq!(buffer[(5, 3)].symbol(), "B");
    assert_eq!(buffer[(5, 4)].symbol(), "g");
}

#[test]
fn unread_rows_keep_green_text_and_independent_yellow_attention_marker() {
    let mut unread = item("Group", "3 unread");
    unread.unread = true;
    unread.attention = true;
    let items = vec![unread];
    let area = Rect::new(0, 0, 30, 3);
    let mut buffer = Buffer::empty(area);

    ContactList::new(&items).render(
        area,
        &mut buffer,
        &mut ListState::default().with_selected(Some(0)),
    );

    assert_eq!(buffer[(5, 0)].symbol(), "@");
    assert_eq!(buffer[(7, 0)].symbol(), "G");
    assert_eq!(buffer[(7, 1)].symbol(), "3");
    assert_eq!(buffer[(5, 0)].fg, Color::Yellow);
    assert_eq!(buffer[(7, 0)].fg, Color::Green);
    assert_eq!(buffer[(7, 1)].fg, Color::Green);
    assert!(buffer[(5, 0)].modifier.contains(Modifier::BOLD));
    assert!(buffer[(7, 0)].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[(7, 0)].bg, Color::DarkGray);
}

#[test]
fn zero_narrow_and_one_row_areas_do_not_panic_or_move_by_terminal_row() {
    let items = vec![item("Alice", "hello"), item("Bob", "goodbye")];
    for area in [
        Rect::new(0, 0, 0, 0),
        Rect::new(0, 0, 1, 1),
        Rect::new(0, 0, 3, 2),
    ] {
        let mut state = ListState::default().with_selected(Some(1));
        let mut buffer = Buffer::empty(area);
        ContactList::new(&items).render(area, &mut buffer, &mut state);
        assert_eq!(state.selected(), Some(1));
    }
}

#[test]
fn item_uses_latest_message_preview_and_local_time_without_an_unread_counter() {
    let mut app = TestApp::new();
    let chat = JID::from("chat@example.test".to_owned());
    app.contacts.insert(chat.clone(), "Alice Example".into());
    app.chat_messages
        .insert(chat.clone(), vec!["new".into(), "old".into()]);
    app.messages
        .insert("old".into(), message(&chat, "old", 60, "older"));
    app.messages
        .insert("new".into(), message(&chat, "new", 120, "newest"));

    let item = ContactListItem::from_chat(&app, &chat);

    assert_eq!(item.name, "Alice Example");
    assert_eq!(item.preview, "newest");
    assert!(item.local_time.is_some());
}

#[test]
fn search_list_renders_the_same_two_row_item_and_preserves_its_selection() {
    let mut app = TestApp::new();
    let hidden = JID::from("hidden@example.test".to_owned());
    let visible = JID::from("visible@example.test".to_owned());
    app.contacts.insert(hidden.clone(), "Hidden Contact".into());
    app.contacts
        .insert(visible.clone(), "Visible Contact".into());
    app.sorted_chats = vec![hidden, visible.clone()];
    app.filtered_chats = vec![visible.clone()];
    app.contact_search.input = "visible".to_owned();
    app.chat_list_state.select(Some(0));
    app.chat_messages
        .insert(visible.clone(), vec!["message".into()]);
    app.messages.insert(
        "message".into(),
        message(&visible, "message", 120, "preview"),
    );

    let backend = TestBackend::new(100, 10);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            ui::draw_with_plan(frame, &mut app, &mut media_render_plan)
        })
        .expect("search contacts should render");
    let rows = terminal
        .backend()
        .buffer()
        .content()
        .chunks(100)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>();

    assert_eq!(app.chat_list_state.selected(), Some(0));
    assert!(rows.iter().any(|row| row.contains("Visible Contact")));
    assert!(rows.iter().any(|row| row.contains("preview")));
    assert!(!rows.iter().any(|row| row.contains("Hidden Contact")));
}

#[test]
fn chats_render_one_aggregated_community_row_and_one_normal_chat_row() {
    let mut app = TestApp::new();
    app.selected_section = Section::Chats;
    let first = JID::from("first@g.us".to_owned());
    let second = JID::from("second@g.us".to_owned());
    let normal = JID::from("normal@s.whatsapp.net".to_owned());
    app.contacts.insert(normal.clone(), "Normal Contact".into());
    for chat in [&first, &second, &normal] {
        app.chats.insert(
            chat.clone(),
            Chat {
                jid: chat.clone(),
                last_message_time: Some(if *chat == second { 20 } else { 10 }),
            },
        );
    }
    app.sorted_chats = vec![first, second, normal];
    app.communities = vec![CommunityNode {
        jid: JID::from("community@g.us".to_owned()),
        name: "Project Team".into(),
        is_root: true,
        linked_groups: vec![
            JID::from("first@g.us".to_owned()),
            JID::from("second@g.us".to_owned()),
        ],
        is_joined: true,
        is_default_subgroup: false,
        is_announce: None,
        participant_count: None,
    }];

    let mut terminal = Terminal::new(TestBackend::new(100, 10)).unwrap();
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            ui::draw_with_plan(frame, &mut app, &mut media_render_plan)
        })
        .unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol().to_owned())
        .collect::<String>();

    assert_eq!(app.visible_chat_rows().len(), 2);
    assert_eq!(rendered.matches("Project Team").count(), 1);
    assert!(!rendered.contains("first@g.us"));
    assert!(!rendered.contains("second@g.us"));
    assert!(rendered.contains("Normal Contact"));
}

#[test]
fn visible_chats_preserve_search_geometry_and_selected_contact_rendering() {
    let mut app = TestApp::new();
    let selected = JID::from("selected@example.test".to_owned());
    app.contacts
        .insert(selected.clone(), "Selected Contact".into());
    app.sorted_chats = vec![selected.clone()];
    app.filtered_chats = vec![selected.clone()];
    app.contact_search.input = "sel".to_owned();
    app.contact_search.character_index = 2;
    app.contact_search_active = true;
    app.chat_list_state.select(Some(0));

    let mut terminal = Terminal::new(TestBackend::new(100, 10)).unwrap();
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            ui::draw_with_plan(frame, &mut app, &mut media_render_plan)
        })
        .unwrap();
    let rows = terminal
        .backend()
        .buffer()
        .content()
        .chunks(100)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>();

    assert!(rows.iter().any(|row| row.contains("/sel")));
    assert!(rows.iter().any(|row| row.contains("Selected Contact")));
    assert_eq!(app.chat_list_state.selected(), Some(0));
}

#[test]
fn avatar_requests_are_selected_first_then_visible_then_overscan() {
    let chats = (0..8)
        .map(|index| JID::from(format!("chat-{index}@example.test")))
        .collect::<Vec<_>>();

    assert_eq!(
        prioritized_avatar_requests(&chats, Some(6), 3, 2),
        vec![
            chats[6].clone(),
            chats[3].clone(),
            chats[4].clone(),
            chats[0].clone(),
            chats[1].clone(),
            chats[2].clone(),
            chats[5].clone(),
            chats[7].clone()
        ]
    );
}

#[test]
fn avatar_runtime_owns_scheduling_while_ui_renderers_only_plan_and_paint() {
    for symbol in [
        "render_contacts",
        "visible_contact_rows",
        "AVATAR_WIDTH",
        "AVATAR_HEIGHT",
        "protocol_mut",
    ] {
        assert!(
            CONTACTS_SOURCE.contains(symbol),
            "contacts renders {symbol}"
        );
        assert_eq!(UI_SOURCE.matches(&format!("fn {symbol}")).count(), 0);
    }
    assert!(CONTACTS_SOURCE.contains("if avatar_area.width == AVATAR_WIDTH"));
    assert!(CONTACTS_SOURCE.contains("&& avatar_area.height == AVATAR_HEIGHT"));
    assert!(!CONTACTS_SOURCE.contains("contact_avatars.schedule"));
    assert!(!COMMUNITIES_SOURCE.contains("contact_avatars.schedule"));
    assert!(!UI_SOURCE.contains("contact_avatars.clear_window"));
    assert!(AVATAR_RUNTIME_SOURCE.contains("schedule_avatar_viewport"));
    assert!(!CONTACTS_SOURCE.contains("pub fn"));
}

#[test]
fn hidden_chats_clear_avatar_window_in_runtime_before_pure_rendering() {
    let mut app = TestApp::new();
    app.selected_section = Section::Chats;
    app.pane_visibility = PaneVisibility {
        section_rail: true,
        chat_list: false,
    };
    app.sorted_chats = vec![JID::from("hidden@example.test".to_owned())];

    let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            ui::draw_with_plan(frame, &mut app, &mut media_render_plan)
        })
        .unwrap();
    assert!(
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|cell| cell.symbol() == "C")
    );
    assert!(UI_SOURCE.contains("if let Some(area) = areas.chat_list"));
    assert!(AVATAR_RUNTIME_SOURCE.contains("schedule_avatar_viewport"));
    assert!(AVATAR_RUNTIME_SOURCE.contains("Vec::new()"));
}

#[test]
fn composition_root_keeps_chats_before_overlays() {
    let draw_source = UI_SOURCE
        .split_once("pub fn draw")
        .and_then(|(_, source)| source.split_once("pub fn render_chats"))
        .map(|(source, _)| source)
        .expect("draw source should remain in ui.rs");
    let order = [
        "render_contacts",
        "render_chats",
        "render_attachment_viewer",
        "render_url_picker",
        "render_share_picker",
        "render_file_picker",
    ];
    let mut previous = 0;
    for symbol in order {
        let current = draw_source.find(symbol).expect("draw call should remain");
        assert!(current >= previous, "draw order changed at {symbol}");
        previous = current;
    }
}

fn item(name: &str, preview: &str) -> ContactListItem {
    ContactListItem {
        name: name.to_owned(),
        initials: initials(name),
        preview: preview.to_owned(),
        local_time: None,
        unread: false,
        attention: false,
    }
}

fn message(chat: &JID, id: &str, timestamp: i64, text: &str) -> Message {
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: chat.clone(),
            mentions_self: false,
            timestamp,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::Text(text.into()),
    }
}
