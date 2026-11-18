use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{ListState, StatefulWidget},
};
use wp_tui::app::{actions::Section, events::MediaRenderPlan, read_receipts::VisibilityPlan};
use wp_tui::ui::{
    self,
    status_list::{StatusList, StatusListItem},
    theme::Theme,
};

mod common;
use common::TestApp;

fn render(settings: &str, section: Section) -> Buffer {
    render_config(settings, None, section)
}

fn render_config(settings: &str, json: Option<&str>, section: Section) -> Buffer {
    let mut app = TestApp::with_config_files(settings, json);
    app.selected_section = section;
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal
        .draw(|frame| {
            ui::draw_with_plan(
                frame,
                &mut app,
                &mut MediaRenderPlan::default(),
                &mut VisibilityPlan::default(),
            );
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn missing_empty_and_invalid_theme_keep_every_default_cell() {
    for section in [
        Section::Chats,
        Section::Communities,
        Section::Status,
        Section::News,
    ] {
        let default = render("", section);
        for settings in [
            "mouse=enable\n",
            "theme=\n",
            "theme=unknown\n",
            "theme=Kanagawa\n",
            "theme=kanagawa-wave\n",
        ] {
            assert_eq!(
                render(settings, section),
                default,
                "{section:?}: {settings:?}"
            );
        }
    }
}

#[test]
fn named_themes_supply_shared_text_surface_and_focus_roles_in_every_section() {
    for (name, text, surface, accent) in [
        ("kanagawa", 0xdcd7ba, 0x1f1f28, 0x7e9cd8),
        ("one-dark", 0xabb2bf, 0x282c34, 0x4aa5f0),
        ("catppuccin", 0xcdd6f4, 0x1e1e2e, 0x89b4fa),
        ("tokyo-night", 0xa9b1d6, 0x1a1b26, 0x7aa2f7),
        ("gruvbox", 0xebdbb2, 0x282828, 0x83a598),
    ] {
        let rgb = |hex: u32| Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
        for section in [
            Section::Chats,
            Section::Communities,
            Section::Status,
            Section::News,
        ] {
            let buffer = render(&format!(" theme = {name} \nmouse=enable\n"), section);
            let y = match section {
                Section::Chats => 1,
                Section::News => 8,
                _ => 0,
            };
            // Focused list border and ordinary conversation text use the
            // resolved theme, including when the conversation is empty.
            assert_eq!(buffer[(14, y)].fg, rgb(accent), "{name} {section:?}");
            assert_eq!(buffer[(14, y)].bg, rgb(surface));
            assert!(buffer.content().iter().any(|cell| {
                !cell.symbol().trim().is_empty() && cell.fg == rgb(text) && cell.bg == rgb(surface)
            }));
            assert_eq!(buffer[(99, 23)].bg, rgb(surface));
        }
    }
}

#[test]
fn custom_bootstrap_renders_overridden_text_surface_and_inherited_base_accent() {
    let json = r##"{"base":"tokyo-night","colors":{"text":"#Ab12eF","surface":"#102030"}}"##;
    let buffer = render_config("theme=custom\n", Some(json), Section::Chats);
    assert_eq!(buffer[(14, 1)].fg, Color::Rgb(0x7a, 0xa2, 0xf7));
    assert_eq!(buffer[(14, 1)].bg, Color::Rgb(0x10, 0x20, 0x30));
    assert!(buffer.content().iter().any(|cell| {
        !cell.symbol().trim().is_empty()
            && cell.fg == Color::Rgb(0xab, 0x12, 0xef)
            && cell.bg == Color::Rgb(0x10, 0x20, 0x30)
    }));
    // The unused bottom-right cell must still have the opaque themed canvas.
    assert_eq!(buffer[(99, 23)].bg, Color::Rgb(0x10, 0x20, 0x30));
}

#[test]
fn custom_low_contrast_and_rgb_boundaries_remain_user_choices() {
    for (value, color) in [
        ("#000000", Color::Rgb(0, 0, 0)),
        ("#FFFFFF", Color::Rgb(255, 255, 255)),
    ] {
        let json =
            format!(r#"{{"colors":{{"text":"{value}","surface":"{value}","accent":"{value}"}}}}"#);
        let buffer = render_config("theme=custom\n", Some(&json), Section::Chats);
        assert_eq!(buffer[(14, 1)].fg, color);
        assert_eq!(buffer[(14, 1)].bg, color);
        assert_eq!(buffer[(99, 23)].bg, color);
        assert!(buffer.content().iter().any(|cell| {
            !cell.symbol().trim().is_empty() && cell.fg == color && cell.bg == color
        }));
    }
}

#[test]
fn rejected_or_missing_custom_file_renders_every_legacy_cell() {
    let default = render("", Section::Chats);
    for json in [
        None,
        Some("malformed json"),
        Some(r##"{"base":"missing","colors":{"surface":"#123456"}}"##),
        Some(r##"{"colors":{"surface":"#123456","text":"#12345678"}}"##),
        Some(r##"{"colors":{"surface":"#123456","unknown":"#abcdef"}}"##),
        Some(r##"{"colors":{"surface":"#123456"},"include":"other.json"}"##),
    ] {
        assert_eq!(
            render_config("theme=custom\n", json, Section::Chats),
            default,
            "{json:?}"
        );
    }
}

#[test]
fn sibling_custom_file_does_not_change_named_or_default_rendering() {
    let json = r##"{"colors":{"text":"#123456","surface":"#abcdef"}}"##;
    for settings in [
        "",
        "theme=kanagawa\n",
        "theme=one-dark\n",
        "theme=catppuccin\n",
        "theme=tokyo-night\n",
        "theme=gruvbox\n",
    ] {
        assert_eq!(
            render_config(settings, Some(json), Section::Chats),
            render(settings, Section::Chats),
            "{settings:?}"
        );
    }
}

#[test]
fn themed_status_rows_keep_selection_viewport_and_unseen_marker() {
    let items = [
        StatusListItem {
            name: "Seen".into(),
            time: "now".into(),
            unseen: false,
        },
        StatusListItem {
            name: "Unseen".into(),
            time: "now".into(),
            unseen: true,
        },
        StatusListItem {
            name: "Selected".into(),
            time: "now".into(),
            unseen: true,
        },
    ];
    let area = Rect::new(0, 0, 24, 2);
    for name in [
        "kanagawa",
        "one-dark",
        "catppuccin",
        "tokyo-night",
        "gruvbox",
    ] {
        let mut buffer = Buffer::empty(area);
        let mut state = ListState::default().with_selected(Some(2));
        StatusList::new(&items)
            .theme(Theme::from_name(name))
            .render(area, &mut buffer, &mut state);
        assert_eq!(state.selected(), Some(2));
        assert_eq!(state.offset(), 1);
        assert_eq!(buffer[(0, 0)].symbol(), "●");
        assert_eq!(buffer[(2, 0)].symbol(), "U");
        assert_eq!(buffer[(2, 1)].symbol(), "S");
        assert_ne!(buffer[(2, 0)].bg, buffer[(2, 1)].bg);
        assert!(buffer[(2, 0)].modifier.contains(Modifier::BOLD));
        assert!(buffer.content().iter().all(|cell| {
            matches!(cell.fg, Color::Rgb(..)) && matches!(cell.bg, Color::Rgb(..))
        }));
    }

    // Omission preserves the pre-theme status styles, including inherited
    // ordinary colors and the legacy selected green-on-dark-gray row.
    let mut buffer = Buffer::empty(area);
    buffer.set_style(area, Style::default().fg(Color::Magenta).bg(Color::Blue));
    let mut state = ListState::default().with_selected(Some(2));
    StatusList::new(&items).render(area, &mut buffer, &mut state);
    assert_eq!(state.offset(), 1);
    assert_eq!(buffer[(2, 0)].fg, Color::Magenta);
    assert_eq!(buffer[(2, 0)].bg, Color::Blue);
    assert_eq!(buffer[(2, 1)].fg, Color::Green);
    assert_eq!(buffer[(2, 1)].bg, Color::DarkGray);
}
