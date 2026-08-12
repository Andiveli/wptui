use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color};
use whatsrust::JID;
use wp_tui::{
    app::{
        App,
        actions::{PaneVisibility, Section},
        composer::PendingAttachment,
    },
    ui::{
        self, attachment_preview_lines, composer_height, composer_visual_cursor,
        composer_visual_rows, conversation_areas, navigation_areas,
    },
};
mod common;
use common::TestApp;

fn draw_top_row(width: u16, setup: impl FnOnce(&mut App)) -> String {
    let mut app = TestApp::new();
    let chat = JID::from("chat@example.test".to_owned());
    app.contacts.insert(chat.clone(), "Alice".into());
    app.sorted_chats.push(chat.clone());
    app.open_chat = Some(chat);
    app.chat_list_state.select(Some(0));
    setup(&mut app);

    let backend = TestBackend::new(width, 6);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| ui::draw(frame, &mut app))
        .expect("chat should render");

    terminal
        .backend()
        .buffer()
        .content()
        .chunks(width as usize)
        .next()
        .unwrap_or_default()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn conversation_presence_marker_precedes_name_with_expected_colors() {
    let mut app = TestApp::new();
    let chat = JID::from("alice@s.whatsapp.net".to_owned());
    app.contacts.insert(chat.clone(), "Alice".into());
    app.sorted_chats.push(chat.clone());
    app.open_chat = Some(chat.clone());
    app.chat_list_state.select(Some(0));
    app.pane_visibility = PaneVisibility {
        section_rail: false,
        chat_list: false,
    };
    app.selected_presence
        .select(Some(chat.clone()), wp_tui::app::unix_now());
    app.selected_presence
        .update(&chat, false, 0, wp_tui::app::unix_now());

    let backend = TestBackend::new(30, 6);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| ui::draw(frame, &mut app))
        .expect("chat should render");
    let row = &terminal.backend().buffer().content()[..30];
    let marker = row.iter().position(|cell| cell.symbol() == "●").unwrap();
    let name = row.iter().position(|cell| cell.symbol() == "A").unwrap();
    assert!(marker < name);
    assert_eq!(row[marker].fg, Color::Green);

    app.selected_presence
        .update(&chat, true, 0, wp_tui::app::unix_now());
    terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
    let row = &terminal.backend().buffer().content()[..30];
    let marker = row.iter().position(|cell| cell.symbol() == "●").unwrap();
    assert_eq!(row[marker].fg, Color::Yellow);
}

#[test]
fn group_conversation_presence_is_empty_circle() {
    let mut app = TestApp::new();
    let group = JID::from("friends@g.us".to_owned());
    app.contacts.insert(group.clone(), "Friends".into());
    app.sorted_chats.push(group.clone());
    app.open_chat = Some(group);
    app.chat_list_state.select(Some(0));
    app.pane_visibility = PaneVisibility {
        section_rail: false,
        chat_list: false,
    };

    let backend = TestBackend::new(30, 6);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
    let row = terminal.backend().buffer().content()[..30]
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(row.contains("○ Friends"), "unexpected header: {row:?}");
}

#[test]
fn composer_height_is_bounded_for_short_and_tall_terminals() {
    assert_eq!(composer_height(1, 0, 0, 4), 1);
    assert_eq!(composer_height(8, 1, 1, 2), 6);
    assert_eq!(composer_height(20, 8, 1, 4), 12);
}

#[test]
fn composer_wraps_visually_without_mutating_its_logical_lines() {
    let lines = vec!["abcde".to_owned()];

    assert_eq!(composer_visual_rows(&lines, 4), 2);
    assert_eq!(composer_visual_cursor(&lines, (0, 5), 4), (1, 1));
    assert_eq!(lines, ["abcde"]);
}

#[test]
fn empty_composer_input_keeps_one_visual_row_and_origin_cursor() {
    let lines = vec![String::new()];

    assert_eq!(composer_visual_rows(&lines, 8), 1);
    assert_eq!(composer_visual_cursor(&lines, (0, 0), 8), (0, 0));
}

#[test]
fn composer_cursor_follows_a_word_moved_to_the_next_visual_row() {
    let width = 8;
    let mut line = "hello wo".to_owned();

    assert_eq!(
        composer_visual_cursor(&[line.clone()], (0, 8), width),
        (1, 0)
    );

    for (character, expected_cursor) in [('r', (1, 3)), ('l', (1, 4)), ('d', (1, 5))] {
        line.push(character);
        assert_eq!(composer_visual_rows(&[line.clone()], width), 2);
        assert_eq!(
            composer_visual_cursor(&[line.clone()], (0, line.chars().count()), width),
            expected_cursor,
            "after typing {line:?}"
        );
    }

    assert_eq!(line, "hello world");
}

#[test]
fn composer_visual_layout_uses_display_width_and_handles_narrow_areas() {
    let lines = vec!["a界b".to_owned(), "c".to_owned()];

    assert_eq!(composer_visual_rows(&lines, 3), 3);
    assert_eq!(composer_visual_cursor(&lines, (0, 3), 3), (1, 1));
    assert_eq!(
        composer_visual_cursor(&["界".to_owned()], (0, 1), 1),
        (0, 0)
    );
    assert_eq!(composer_visual_rows(&lines, 0), 2);
}

#[test]
fn composer_height_accounts_for_wrapped_rows_and_cursor_at_the_edge() {
    let lines = vec!["abcd".to_owned()];
    let cursor = composer_visual_cursor(&lines, (0, 4), 4);
    let rows = composer_visual_rows(&lines, 4).max(cursor.0 + 1);

    assert_eq!(cursor, (1, 0));
    assert_eq!(composer_height(20, rows, 0, 0), 4);
}

#[test]
fn composer_renders_wrapped_text_without_inserting_a_newline() {
    let mut app = TestApp::new();
    let chat = JID::from("chat@example.test".to_owned());
    app.sorted_chats.push(chat.clone());
    app.open_chat = Some(chat);
    app.chat_list_state.select(Some(0));
    app.conversation_mode = wp_tui::app::actions::ConversationMode::ComposerEditing;
    app.pane_visibility = PaneVisibility {
        section_rail: false,
        chat_list: false,
    };
    app.composer.insert_text("abcdefghi");

    let backend = TestBackend::new(12, 8);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| ui::draw(frame, &mut app))
        .expect("chat should render");

    let rows = terminal
        .backend()
        .buffer()
        .content()
        .chunks(12)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>();
    assert_eq!(
        rows[4].chars().skip(2).take(8).collect::<String>(),
        "abcdefgh"
    );
    assert_eq!(
        rows[5].chars().skip(2).take(8).collect::<String>(),
        "i       "
    );
    assert_eq!(app.composer.text(), "abcdefghi");
}

#[test]
fn composer_renders_a_word_on_the_same_row_as_its_cursor() {
    let mut app = TestApp::new();
    let chat = JID::from("chat@example.test".to_owned());
    app.sorted_chats.push(chat.clone());
    app.open_chat = Some(chat);
    app.chat_list_state.select(Some(0));
    app.conversation_mode = wp_tui::app::actions::ConversationMode::ComposerEditing;
    app.pane_visibility = PaneVisibility {
        section_rail: false,
        chat_list: false,
    };
    app.composer.insert_text("hello world");

    let backend = TestBackend::new(12, 8);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| ui::draw(frame, &mut app))
        .expect("chat should render");

    let rows = terminal
        .backend()
        .buffer()
        .content()
        .chunks(12)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>();
    assert_eq!(
        rows[4].chars().skip(2).take(8).collect::<String>(),
        "hello   "
    );
    assert_eq!(
        rows[5].chars().skip(2).take(8).collect::<String>(),
        "world   "
    );
    assert_eq!(
        composer_visual_cursor(
            app.composer.input.lines(),
            (app.composer.input.cursor().0, app.composer.input.cursor().1),
            8,
        ),
        (1, 5)
    );
    assert_eq!(app.composer.text(), "hello world");
}

#[test]
fn composer_scrolls_to_keep_the_selected_cursor_row_visible() {
    let mut app = TestApp::new();
    let chat = JID::from("chat@example.test".to_owned());
    app.sorted_chats.push(chat.clone());
    app.open_chat = Some(chat);
    app.chat_list_state.select(Some(0));
    app.conversation_mode = wp_tui::app::actions::ConversationMode::ComposerEditing;
    app.pane_visibility = PaneVisibility {
        section_rail: false,
        chat_list: false,
    };
    app.composer.insert_text("one\ntwo\nthree\nfour\nfive");

    let backend = TestBackend::new(20, 8);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| ui::draw(frame, &mut app))
        .expect("chat should render");

    let rows = terminal
        .backend()
        .buffer()
        .content()
        .chunks(20)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>();

    assert!(rows.iter().any(|row| row.contains("four")), "{rows:?}");
    assert!(rows.iter().any(|row| row.contains("five")), "{rows:?}");
    assert!(!rows.iter().any(|row| row.contains("one")), "{rows:?}");
    assert!(!rows.iter().any(|row| row.contains("two")), "{rows:?}");
    assert!(!rows.iter().any(|row| row.contains("three")), "{rows:?}");
}

#[test]
fn conversation_areas_keep_the_message_area_valid_in_short_terminals() {
    let (messages, composer) = conversation_areas(Rect::new(0, 0, 20, 4), 1, 1, 4);

    assert_eq!(messages, Rect::new(0, 0, 20, 1));
    assert_eq!(composer, Rect::new(0, 2, 20, 2));
}

#[test]
fn each_pending_attachment_has_an_inline_preview_line() {
    let attachments = vec![
        PendingAttachment::new("/tmp/first.png".into(), whatsrust::FileKind::Image),
        PendingAttachment::new("/tmp/second.pdf".into(), whatsrust::FileKind::Document),
        PendingAttachment::new("/tmp/third.mp3".into(), whatsrust::FileKind::Audio),
    ];

    assert_eq!(
        attachment_preview_lines(&attachments),
        vec![
            "Image: first.png",
            "Document: second.pdf",
            "Audio: third.mp3"
        ]
    );
}

#[test]
fn navigation_layout_assigns_remaining_width_to_conversation() {
    let area = Rect::new(0, 0, 100, 20);

    let all = navigation_areas(area, PaneVisibility::default());
    assert_eq!(all.section_rail, Some(Rect::new(0, 0, 14, 20)));
    assert_eq!(all.chat_list, Some(Rect::new(14, 0, 30, 20)));
    assert_eq!(all.conversation, Rect::new(44, 0, 56, 20));

    let hidden = navigation_areas(
        area,
        PaneVisibility {
            section_rail: false,
            chat_list: false,
        },
    );
    assert_eq!(hidden.section_rail, None);
    assert_eq!(hidden.chat_list, None);
    assert_eq!(hidden.conversation, area);
}

#[test]
fn navigation_layout_supports_each_single_visible_navigation_pane() {
    let area = Rect::new(0, 0, 100, 20);

    let rail_only = navigation_areas(
        area,
        PaneVisibility {
            section_rail: true,
            chat_list: false,
        },
    );
    assert_eq!(rail_only.section_rail, Some(Rect::new(0, 0, 14, 20)));
    assert_eq!(rail_only.chat_list, None);
    assert_eq!(rail_only.conversation, Rect::new(14, 0, 86, 20));

    let chat_list_only = navigation_areas(
        area,
        PaneVisibility {
            section_rail: false,
            chat_list: true,
        },
    );
    assert_eq!(chat_list_only.section_rail, None);
    assert_eq!(chat_list_only.chat_list, Some(Rect::new(0, 0, 30, 20)));
    assert_eq!(chat_list_only.conversation, Rect::new(30, 0, 70, 20));
}

#[test]
fn structural_placeholders_do_not_render_chat_or_contact_content() {
    // Communities now renders its approved hierarchy and real chat pane.
    for section in [Section::Communities] {
        let mut app = TestApp::new();
        let chat = JID::from("chat@example.test".to_owned());
        app.contacts
            .insert(chat.clone(), "CHAT_CONTACT_SENTINEL".into());
        app.sorted_chats.push(chat);
        app.selected_section = section;

        let backend = TestBackend::new(100, 12);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| ui::draw(frame, &mut app))
            .expect("placeholder should render");

        let output = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(output.contains("Communities"));
        assert!(!output.contains("CHAT_CONTACT_SENTINEL"));
        assert!(!output.contains("Contacts"));
        assert!(!output.contains("Message input"));
    }
}

#[test]
fn navigation_layout_remains_valid_in_narrow_terminals() {
    for width in 0..=20 {
        let area = Rect::new(3, 2, width, 5);
        let panes = navigation_areas(area, PaneVisibility::default());

        assert!(
            panes
                .section_rail
                .is_none_or(|pane| pane.right() <= area.right())
        );
        assert!(
            panes
                .chat_list
                .is_none_or(|pane| pane.right() <= area.right())
        );
        assert!(panes.conversation.right() <= area.right());
    }
}

#[test]
fn chat_border_keeps_contact_name_when_no_notice_is_active() {
    let row = draw_top_row(80, |_| {});

    assert!(row.contains("Alice"), "missing left-aligned name: {row:?}");
    assert!(
        !row.contains("Copied"),
        "no status should leak when action_notice is None: {row:?}"
    );
}

#[test]
fn chat_border_places_action_notice_right_next_to_the_contact_name() {
    let row = draw_top_row(80, |app| {
        app.action_notice = Some(wp_tui::app::actions::ActionNotice::CopiedText(
            "hello".into(),
        ));
    });

    let name = row.find("Alice").expect("name on the left");
    let notice = row.find("Copied").expect("notice on the right");
    assert!(
        name < notice,
        "name should be to the left of notice: {row:?}"
    );
}

#[test]
fn chat_border_truncates_long_action_notice_with_ellipsis() {
    let payload = "x".repeat(120);
    let row = draw_top_row(60, |app| {
        app.action_notice = Some(wp_tui::app::actions::ActionNotice::CopiedText(payload));
    });

    assert!(row.contains("Alice"), "name still on the left: {row:?}");
    assert!(row.contains("…"), "notice should be truncated: {row:?}");
}

#[test]
fn chat_border_truncates_unicode_action_notice_within_cell_width() {
    let payload = "界".repeat(60);
    let row = draw_top_row(100, |app| {
        app.action_notice = Some(wp_tui::app::actions::ActionNotice::CopiedText(payload));
    });

    assert!(row.contains("界"), "{row:?}");
    assert!(row.contains("…"), "notice should be truncated: {row:?}");
    assert!(row.matches("界").count() <= 40, "{row:?}");
}
