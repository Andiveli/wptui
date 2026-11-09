//! Deterministic TestBackend cell proof, not a terminal image-protocol capture.
//! Runs without an account or media worker. Normal runs write only TestApp's
//! disposable state. CI can explicitly export the cell grid for PNG conversion.
use std::fmt::Write as _;

use ratatui::{Terminal, backend::TestBackend};
use whatsrust as wr;
use wp_tui::{
    app::{actions::FocusPane, events::MediaRenderPlan, read_receipts::VisibilityPlan},
    ui,
};

mod common;
use common::TestApp;

const FIXTURES: &[(&str, &str)] = &[
    ("incoming", "Read https://example.test/article?q=%2F#part"),
    (
        "repeated",
        "https://example.test/repeated then https://example.test/repeated",
    ),
    (
        "cjk",
        "阿丽分享 日本語 https://example.test/文章?言語=日本語",
    ),
    (
        "narrow",
        "A longer incoming body wraps naturally before https://example.test/a/long/path?exact=%2Fvalue#section and keeps the original destination.",
    ),
];

fn fixture(body: &str) -> TestApp {
    let mut app = TestApp::new();
    let chat = wr::JID::from("fixture@example.test".to_owned());
    app.contacts.insert(chat.clone(), "Fixture".into());
    app.open_chat = Some(chat.clone());
    app.focus_pane = FocusPane::Conversation;
    let message = wr::Message {
        info: wr::MessageInfo {
            id: "fixture-message".into(),
            chat: chat.clone(),
            sender: chat,
            mentions_self: false,
            timestamp: 1_704_110_400,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: wr::MessageContent::Text(body.into()),
    };
    let urls = wp_tui::url::extract_openable_urls(body);
    wr::store_native_link_preview(
        &message.info,
        body,
        wr::NativeLinkPreview {
            matched_text: urls[0].as_str().into(),
            title: "Native fixture title".into(),
            description: "Supplied description, no website lookup.".into(),
            jpeg_thumbnail: Vec::new(),
        },
    );
    // T1 GREEN can capture metadata-bearing messages in the unchanged old UI;
    // the identical fixture then reveals T2's text-only preview presentation.
    app.process_message(message, true);
    app.chat_list_state.select(Some(0));
    app
}

fn json_string(value: &str) -> String {
    let mut escaped = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c.is_control() => write!(escaped, "\\u{:04x}", c as u32).unwrap(),
            c => escaped.push(c),
        }
    }
    escaped.push('"');
    escaped
}

fn capture(name: &str, body: &str, width: u16, height: u16) -> String {
    let mut app = fixture(body);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            let mut media = MediaRenderPlan::default();
            let mut visibility = VisibilityPlan::default();
            // Plans are collected but deliberately never executed.
            ui::draw_with_plan(frame, &mut app, &mut media, &mut visibility);
        })
        .unwrap();
    assert!(matches!(
        &app.messages["fixture-message"].message,
        wr::MessageContent::Text(source) if source.as_ref() == body
    ));
    let buffer = terminal.backend().buffer();
    assert_eq!(
        buffer.content().len(),
        usize::from(width) * usize::from(height)
    );
    assert!(buffer.content().iter().any(|cell| cell.symbol() != " "));
    let mut json = format!(
        "{{\"kind\":\"ratatui-TestBackend-cells\",\"fixture\":{},\"source\":{},\"width\":{width},\"height\":{height},\"cells\":[",
        json_string(name),
        json_string(body),
    );
    for y in 0..height {
        for x in 0..width {
            if x != 0 || y != 0 {
                json.push(',');
            }
            let cell = &buffer[(x, y)];
            write!(
                json,
                "{{\"x\":{x},\"y\":{y},\"symbol\":{},\"fg\":{},\"bg\":{},\"style\":{}}}",
                json_string(cell.symbol()),
                json_string(&format!("{:?}", cell.fg)),
                json_string(&format!("{:?}", cell.bg)),
                json_string(&format!("{:?}", cell.style())),
            )
            .unwrap();
        }
    }
    json.push_str("]}\n");
    json
}

#[test]
fn incoming_link_fixture_cells_are_repeatable_at_shipped_sizes() {
    let export = std::env::var("WPTUI_CAPTURE_PREVIEW_SNAPSHOTS").as_deref() == Ok("1");
    for &(name, body) in FIXTURES {
        for (width, height) in [(180, 50), (140, 40), (80, 24)] {
            let first = capture(name, body, width, height);
            assert_eq!(
                first,
                capture(name, body, width, height),
                "{name} {width}x{height}"
            );
            if export {
                // Fixed ignored output only; never accept an arbitrary path.
                let directory = std::path::Path::new("target/preview-snapshots");
                std::fs::create_dir_all(directory).unwrap();
                std::fs::write(
                    directory.join(format!("{name}-{width}x{height}.json")),
                    first,
                )
                .unwrap();
            }
        }
    }
}
