use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers},
    style::Color,
};
use wp_tui::app::{
    actions::{ActionNotice, FocusPane, Section},
    contextual_actions::{ContextualAction, RowStyle},
    events::MediaRenderPlan,
    read_receipts::VisibilityPlan,
};
use wp_tui::ui;

mod common;
use common::TestApp;

const LABELS: [&str; 7] = [
    "Default",
    "Kanagawa",
    "One Dark",
    "Catppuccin",
    "Tokyo Night",
    "Gruvbox",
    "Custom",
];
const CUSTOM: &str = r##"{"base":"tokyo-night","colors":{"text":"#Ab12eF","surface":"#102030"}}"##;

fn press(app: &mut TestApp, code: KeyCode) {
    app.on_terminal_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn render(app: &mut TestApp) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| {
            ui::draw_with_plan(
                frame,
                app,
                &mut MediaRenderPlan::default(),
                &mut VisibilityPlan::default(),
            );
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn lines(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn settings(app: &TestApp) -> String {
    std::fs::read_to_string(app.settings_path()).unwrap()
}

fn open_themes(app: &mut TestApp) {
    press(app, KeyCode::Char(' '));
    press(app, KeyCode::Char('a'));
    let actions = lines(&render(app));
    assert!(actions.iter().any(|line| line.contains("Actions")));
    assert!(actions.iter().any(|line| line.contains("t  Themes")));
    press(app, KeyCode::Char('t'));
    let chooser = lines(&render(app));
    assert!(chooser.iter().any(|line| line.contains("Themes")));
    let visible = chooser
        .iter()
        .filter_map(|line| LABELS.iter().find(|label| line.contains(**label)).copied())
        .collect::<Vec<_>>();
    assert_eq!(visible, LABELS, "all seven choices appear once, in order");
}

// Starting at Default, exercise both directions without relying on wraparound.
fn choose_from_default(app: &mut TestApp, index: usize, vim: bool) {
    open_themes(app);
    let down = if vim {
        KeyCode::Char('j')
    } else {
        KeyCode::Down
    };
    let up = if vim { KeyCode::Char('k') } else { KeyCode::Up };
    for _ in 0..index.max(1) {
        press(app, down);
    }
    press(app, up);
    if index > 0 {
        press(app, down);
    }
    press(app, KeyCode::Enter);
}

fn assert_palette(buffer: &Buffer, text: Color, surface: Color, accent: Color) {
    assert_eq!(buffer[(14, 1)].fg, accent);
    assert_eq!(buffer[(14, 1)].bg, surface);
    assert_eq!(buffer[(99, 29)].bg, surface);
    assert!(
        buffer.content().iter().any(|cell| {
            !cell.symbol().trim().is_empty() && cell.fg == text && cell.bg == surface
        })
    );
}

fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[test]
fn themes_are_global_without_a_selected_message_inside_space_a() {
    for focus in [
        FocusPane::SectionRail,
        FocusPane::ChatList,
        FocusPane::Conversation,
    ] {
        let mut app = TestApp::with_settings("mouse=disable\n");
        app.focus_pane = focus;
        let before = settings(&app);
        open_themes(&mut app);
        press(&mut app, KeyCode::Esc);
        assert_eq!(settings(&app), before);
        assert!(!app.should_quit);
    }
}

#[test]
fn arrows_and_jk_apply_each_builtin_immediately_and_persist_without_restart() {
    for vim in [false, true] {
        for (index, name, text, surface, accent) in [
            (1, "kanagawa", 0xdcd7ba, 0x1f1f28, 0x7e9cd8),
            (2, "one-dark", 0xabb2bf, 0x282c34, 0x4aa5f0),
            (3, "catppuccin", 0xcdd6f4, 0x1e1e2e, 0x89b4fa),
            (4, "tokyo-night", 0xa9b1d6, 0x1a1b26, 0x7aa2f7),
            (5, "gruvbox", 0xebdbb2, 0x282828, 0x83a598),
        ] {
            let mut app = TestApp::with_settings("mouse=disable\ncustom=keep\n");
            choose_from_default(&mut app, index, vim);
            let saved = settings(&app);
            let selections = saved
                .lines()
                .filter_map(|line| line.split_once('='))
                .filter(|(key, _)| key.trim() == "theme")
                .map(|(_, value)| value.trim())
                .collect::<Vec<_>>();
            assert_eq!(selections, [name]);
            assert!(saved.lines().any(|line| line == "mouse=disable"));
            assert!(saved.lines().any(|line| line == "custom=keep"));
            assert_palette(&render(&mut app), rgb(text), rgb(surface), rgb(accent));
            assert!(!matches!(
                app.action_notice,
                Some(ActionNotice::Unavailable(_))
            ));
        }
    }
}

#[test]
fn escape_after_navigation_keeps_the_palette_and_settings_unchanged() {
    let mut app = TestApp::with_settings("theme=kanagawa\ncustom=keep\n");
    let before = render(&mut app);
    let saved = settings(&app);
    open_themes(&mut app);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(settings(&app), saved, "navigation alone must not save");
    assert_eq!(render(&mut app)[(99, 29)].bg, before[(99, 29)].bg);
    press(&mut app, KeyCode::Esc);
    assert_eq!(render(&mut app), before);
    assert_eq!(settings(&app), saved);
    assert!(app.action_notice.is_none());
}

#[test]
fn default_removes_the_effect_of_every_duplicate_old_theme_selection() {
    let original = "theme=kanagawa\n theme = gruvbox\ntheme=custom\ncustom=keep\n";
    let mut app = TestApp::with_settings(original);
    open_themes(&mut app);
    // The current Kanagawa choice is immediately after Default.
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Enter);
    let saved = settings(&app);
    assert!(saved.lines().any(|line| line == "custom=keep"));
    for (key, value) in saved.lines().filter_map(|line| line.split_once('=')) {
        if key.trim() == "theme" {
            assert!(value.trim().is_empty() || value.trim() == "default");
        }
    }
    let mut default = TestApp::with_settings("custom=keep\n");
    let expected = render(&mut default);
    let actual = render(&mut app);
    assert_eq!(actual[(14, 1)], expected[(14, 1)]);
    assert_eq!(actual[(99, 29)], expected[(99, 29)]);
    // Re-bootstrap only verifies saved Default cannot resurrect a duplicate.
    let mut reloaded = TestApp::with_settings(&saved);
    assert_eq!(render(&mut reloaded), expected);
}

#[test]
fn selecting_custom_uses_the_existing_sibling_json_without_rewriting_it() {
    let mut app = TestApp::with_config_files("custom=keep\n", Some(CUSTOM));
    choose_from_default(&mut app, 6, false);
    let saved = settings(&app);
    assert!(saved.lines().any(|line| line == "theme=custom"));
    assert!(saved.lines().any(|line| line == "custom=keep"));
    assert_eq!(
        std::fs::read_to_string(app.settings_path().with_file_name("theme.json")).unwrap(),
        CUSTOM
    );
    assert_palette(
        &render(&mut app),
        rgb(0xab12ef),
        rgb(0x102030),
        rgb(0x7aa2f7),
    );
}

#[test]
fn persistence_failure_reports_the_error_without_applying_or_claiming_success() {
    let mut app = TestApp::with_unwritable_settings();
    let before = render(&mut app);
    choose_from_default(&mut app, 1, false);
    let message = match &app.action_notice {
        Some(ActionNotice::Unavailable(message)) => message.to_lowercase(),
        notice => panic!("expected an honest persistence failure, got {notice:?}"),
    };
    assert!(message.contains("theme"));
    assert!(message.contains("persist") || message.contains("save"));
    assert!(message.contains("could not") || message.contains("failed"));
    let displayed = lines(&render(&mut app)).join("\n").to_lowercase();
    assert!(displayed.contains("theme"));
    assert!(displayed.contains("could not") || displayed.contains("failed"));
    assert!(!displayed.contains("theme applied"));
    assert!(!displayed.contains("theme saved"));
    press(&mut app, KeyCode::Esc);
    let after = render(&mut app);
    assert_eq!(after[(14, 1)], before[(14, 1)]);
    assert_eq!(after[(99, 29)], before[(99, 29)]);
    assert!(app.settings_path().is_dir());
    assert_eq!(
        std::fs::read_to_string(app.settings_path().join("sentinel")).unwrap(),
        "keep\n"
    );
    assert_eq!(std::fs::read_dir(app.settings_path()).unwrap().count(), 1);
}

#[test]
fn space_a_preserves_existing_message_action_availability_and_shortcuts() {
    let mut app = TestApp::new();
    app.focus_pane = FocusPane::Conversation;
    app.selected_section = Section::Chats;
    press(&mut app, KeyCode::Char(' '));
    press(&mut app, KeyCode::Char('a'));
    let rows = &app
        .contextual_menu
        .as_ref()
        .expect("Space+a opens Actions")
        .0;
    for (action, shortcut, style) in [
        (ContextualAction::Copy, 'y', RowStyle::Disabled),
        (ContextualAction::Reply, 'R', RowStyle::Disabled),
        (ContextualAction::Attach, 'a', RowStyle::Enabled),
        (ContextualAction::Quit, 'q', RowStyle::Enabled),
    ] {
        let row = rows.iter().find(|row| row.action_token == action).unwrap();
        assert_eq!(row.display_shortcut, shortcut);
        assert_eq!(row.row_style, style);
    }
    press(&mut app, KeyCode::Char('y'));
    assert!(app.contextual_menu.is_some(), "disabled copy stays guarded");
    assert!(app.action_notice.is_none());
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char(' '));
    press(&mut app, KeyCode::Char('a'));
    press(&mut app, KeyCode::Char('q'));
    assert!(app.should_quit);
    assert!(app.contextual_menu.is_none());
}

#[test]
fn theme_and_notification_menu_choices_preserve_each_others_settings() {
    let original = "# keep\nnotifications=disable\nnotification_sound=system\n";
    let mut app = TestApp::with_settings(original);
    choose_from_default(&mut app, 1, false);
    let palette = render(&mut app);
    assert_eq!(settings(&app), format!("{original}theme=kanagawa\n"));

    press(&mut app, KeyCode::Char(' '));
    press(&mut app, KeyCode::Char('a'));
    assert!(
        lines(&render(&mut app))
            .iter()
            .any(|line| line.contains("n  Notifications: Off"))
    );
    press(&mut app, KeyCode::Char('n'));
    assert!(app.action_notice.is_none());
    assert_eq!(render(&mut app), palette);
    assert_eq!(
        settings(&app),
        "# keep\nnotifications=enable\nnotification_sound=system\ntheme=kanagawa\n"
    );

    press(&mut app, KeyCode::Char(' '));
    press(&mut app, KeyCode::Char('a'));
    assert!(
        lines(&render(&mut app))
            .iter()
            .any(|line| line.contains("n  Notifications: On"))
    );
}

#[test]
fn theme_selector_blocks_mouse_clicks_on_underlying_status_and_logout_rail() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

    for label in ["Status", "Logout"] {
        let mut app = TestApp::with_settings("mouse=enable\ncustom=keep\n");
        let before = render(&mut app);
        let saved = settings(&app);
        let section = app.selected_section;
        let focus = app.focus_pane;
        let pending_logout = app.pending_logout;
        open_themes(&mut app);
        let chooser = render(&mut app);
        let row = lines(&chooser)
            .iter()
            .position(|line| line.chars().take(13).collect::<String>().contains(label))
            .expect("underlying rail entry remains visible beside the selector");
        app.on_terminal_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: row as u16,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(app.selected_section, section, "{label} click leaked");
        assert_eq!(app.focus_pane, focus, "{label} click changed focus");
        assert_eq!(app.pending_logout, pending_logout, "{label} click leaked");
        assert_eq!(settings(&app), saved);
        assert_eq!(render(&mut app), chooser, "{label} click changed the UI");
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.selected_section, section);
        assert_eq!(app.focus_pane, focus);
        assert_eq!(app.pending_logout, pending_logout);
        assert_eq!(settings(&app), saved);
        assert_eq!(render(&mut app), before);
    }
}
