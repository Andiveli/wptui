use std::fs;
use wp_tui::app::read_receipts::VisibilityPlan;

use ratatui::{Terminal, backend::TestBackend};
use whatsrust as wr;
use wp_tui::{
    app::actions::{FocusPane, Section},
    app::events::{AttachmentViewerState, MediaRenderPlan, ViewerAttachment, ViewerStatus},
    file_picker::FilePickerState,
    ui,
};

mod common;
use common::TestApp;

const UI_SOURCE: &str = include_str!("../src/ui.rs");
const NAVIGATION_SOURCE: &str = include_str!("../src/ui/navigation.rs");

fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buffer = terminal.backend().buffer();
    (0..buffer.area().height)
        .map(|y| {
            (0..buffer.area().width)
                .map(|x| buffer[(x, y)].symbol())
                .collect()
        })
        .collect()
}

#[test]
fn navigation_symbols_have_bounded_module_ownership() {
    for symbol in [
        "render_section_rail",
        "render_structural_placeholder",
        "render_logout_placeholder",
        "render_logs",
    ] {
        assert!(
            NAVIGATION_SOURCE.contains(symbol),
            "navigation owns {symbol}"
        );
        assert_eq!(UI_SOURCE.matches(&format!("fn {symbol}")).count(), 0);
    }
    assert!(!NAVIGATION_SOURCE.contains("pub fn"));
}

#[test]
fn draw_keeps_navigation_branch_and_overlay_order() {
    let draw_source = UI_SOURCE
        .split_once("pub fn draw")
        .and_then(|(_, source)| source.split_once("pub fn render_chats"))
        .map(|(source, _)| source)
        .expect("draw source should be present");
    let order = [
        "render_logs",
        "render_section_rail",
        "render_logout_placeholder",
        "render_chats",
        "render_statuses",
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

#[test]
fn navigation_renders_labels_logs_logout_and_placeholders() {
    let mut app = TestApp::new();
    app.focus_pane = FocusPane::SectionRail;
    app.selected_section = Section::Communities;
    app.show_logs = true;

    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            let mut visibility_plan = VisibilityPlan::default();
            ui::draw_with_plan(
                frame,
                &mut app,
                &mut media_render_plan,
                &mut visibility_plan,
            )
        })
        .unwrap();
    let rendered = rows(&terminal).join("\n");
    for expected in [
        "Sections",
        "Chats",
        "Status",
        "Communities",
        "Logs",
        "No communities",
    ] {
        assert!(rendered.contains(expected), "missing {expected}");
    }

    app.rail_on_logout = true;
    app.show_logs = false;
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            let mut visibility_plan = VisibilityPlan::default();
            ui::draw_with_plan(
                frame,
                &mut app,
                &mut media_render_plan,
                &mut visibility_plan,
            )
        })
        .unwrap();
    assert!(
        rows(&terminal)
            .join("\n")
            .contains("Press Enter to sign out")
    );
}

#[test]
fn navigation_is_safe_in_a_narrow_frame() {
    let mut app = TestApp::new();
    app.selected_section = Section::Communities;
    let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            let mut visibility_plan = VisibilityPlan::default();
            ui::draw_with_plan(
                frame,
                &mut app,
                &mut media_render_plan,
                &mut visibility_plan,
            )
        })
        .unwrap();
    assert!(rows(&terminal).join("\n").contains("Sections"));
}

#[test]
fn runtime_overlay_layering_keeps_logs_and_branch_below_final_file_picker() {
    let mut app = TestApp::new();
    app.focus_pane = FocusPane::SectionRail;
    app.selected_section = Section::Communities;
    app.show_logs = true;
    app.attachment_viewer = Some(AttachmentViewerState::from_attachments(
        vec![ViewerAttachment {
            message_id: "overlay-message".into(),
            kind: wr::FileKind::Document,
            path: "attachment-marker.bin".into(),
            status: ViewerStatus::Missing,
        }],
        0,
    ));
    app.url_picker = Some((vec!["url-marker".to_owned()], 0));
    let recipient = "share-marker@s.whatsapp.net".to_owned().into();
    app.share_picker = Some(wp_tui::app::SharePicker::new(
        vec![recipient],
        [(
            "share-marker@s.whatsapp.net".to_owned().into(),
            "share-marker".to_owned(),
        )]
        .into_iter()
        .collect(),
        Default::default(),
    ));

    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("file-marker.txt"), "overlay").unwrap();
    app.file_picker = Some(FilePickerState::open(directory.path()).unwrap());

    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            let mut visibility_plan = VisibilityPlan::default();
            ui::draw_with_plan(
                frame,
                &mut app,
                &mut media_render_plan,
                &mut visibility_plan,
            )
        })
        .unwrap();
    let rendered = rows(&terminal).join("\n");

    assert!(
        rendered.contains("Logs"),
        "logs must remain rendered: {rendered:?}"
    );
    assert!(
        rendered.contains("Communities"),
        "selected branch must remain rendered: {rendered:?}"
    );
    assert!(
        rendered.contains("Attach file:"),
        "final file picker must win over the picker overlays: {rendered:?}"
    );
    assert!(
        rendered.contains("file-marker.txt"),
        "final picker contents must render: {rendered:?}"
    );
    assert!(
        rendered.contains("attachment-marker.bin"),
        "attachment viewer must remain layered below the final picker: {rendered:?}"
    );
    for covered in ["url-marker", "share-marker"] {
        assert!(
            !rendered.contains(covered),
            "later overlay must cover {covered}: {rendered:?}"
        );
    }
}
