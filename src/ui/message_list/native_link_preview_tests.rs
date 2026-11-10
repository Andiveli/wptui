//! Behavioral presentation contract, using only APIs present before T2b.
//! TestBackend cells are not Kitty/Sixel evidence. Plans are never executed.
use std::{
    io::Cursor,
    sync::{Arc, Mutex},
};

use image::{DynamicImage, Rgb, RgbImage};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};
use ratatui_image::picker::Picker;
use whatsrust as wr;

use super::*;
use crate::app::{
    actions::{ConversationMode, FocusPane},
    events::MediaRenderEffect,
    native_link_thumbnails::{EntryState, ThumbnailKey, prepare_thumbnail},
    read_receipts::VisibilityPlan,
    runtime_diagnostics::MessageListCounts,
    test_support::{FixedClock, RecordingNotifier, TestApp, message},
};

const URL: &str = "https://example.test/article?q=%2F#part";
const LABEL: &str = "example.test · article";
const TITLE: &str = "Native title";
const DESCRIPTION: &str = "Supplied description";
const SIZES: [(u16, u16); 3] = [(180, 50), (140, 40), (80, 24)];

fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([240, 30, 40])));
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .unwrap();
    bytes.into_inner()
}

fn fixture(source: &str, thumbnail: Vec<u8>) -> TestApp {
    let mut app = TestApp::with_ports(FixedClock::new(1_704_110_400), RecordingNotifier::default());
    let chat: wr::JID = "fixture@example.test".to_owned().into();
    let mut item = message(&chat, "native-ui", 1_704_110_400);
    item.message = wr::MessageContent::Text(source.into());
    wr::store_native_link_preview(
        &item.info,
        source,
        wr::NativeLinkPreview {
            matched_text: URL.into(),
            title: TITLE.into(),
            description: DESCRIPTION.into(),
            jpeg_thumbnail: thumbnail,
        },
    );
    app.process_message(item, true);
    app.open_chat = Some(chat.clone());
    app.contacts.insert(chat, "Fixture".into());
    app.focus_pane = FocusPane::Conversation;
    app.conversation_mode = ConversationMode::ComposerEditing;
    app.message_list_state.reset();
    app
}

fn keys(plan: MediaRenderPlan) -> Vec<ThumbnailKey> {
    plan.into_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            MediaRenderEffect::LoadNativeLinkThumbnail(key) => Some(key),
            _ => None,
        })
        .collect()
}

fn draw(app: &mut TestApp, size: (u16, u16)) -> (Buffer, Vec<ThumbnailKey>) {
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
    let mut plan = MediaRenderPlan::default();
    terminal
        .draw(|frame| {
            crate::ui::draw_with_plan(frame, app, &mut plan, &mut VisibilityPlan::default())
        })
        .unwrap();
    (terminal.backend().buffer().clone(), keys(plan))
}

fn text(buffer: &Buffer) -> String {
    buffer
        .area
        .rows()
        .map(|row| {
            row.columns()
                .map(|cell| buffer[cell].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn source_and_hits(app: &TestApp, expected: &str) {
    assert!(matches!(&app.messages["native-ui"].message,
        wr::MessageContent::Text(source) if source.as_ref() == expected));
    let hits = &app.mouse_hit_map.urls;
    assert!(!hits.is_empty(), "original URL must remain actionable");
    assert!(
        hits.iter()
            .all(|hit| hit.id.as_ref() == "native-ui" && hit.url == URL)
    );
}

fn hit_cells(app: &TestApp, buffer: &Buffer) -> String {
    app.mouse_hit_map
        .urls
        .iter()
        .map(|hit| buffer[(hit.area.x, hit.area.y)].symbol())
        .collect()
}

fn height(app: &mut TestApp, width: usize) -> usize {
    let item = app.messages["native-ui"].clone();
    message_height(&item, width, false, AuthorGroupContext::STARTS_GROUP, app)
}

fn single_key(app: &mut TestApp, size: (u16, u16)) -> (Buffer, ThumbnailKey) {
    let (buffer, demand) = draw(app, size);
    assert_eq!(
        demand.len(),
        1,
        "one visible native thumbnail, not one per URL/title"
    );
    let key = demand.into_iter().next().unwrap();
    assert_eq!(key.chat, app.messages["native-ui"].info.chat);
    assert_eq!(key.message_id.as_ref(), "native-ui");
    assert_eq!(key.matched_url.as_ref(), URL);
    let wr::MessageContent::Text(source) = &app.messages["native-ui"].message else {
        panic!("text")
    };
    assert_eq!(&key.source, source);
    assert!(
        Arc::ptr_eq(&key.source, source),
        "draw must cheaply clone source Arc"
    );
    assert!(key.size.width > 0 && key.size.height > 0);
    (buffer, key)
}

// Arrange cache completion explicitly, outside draw and without managed workers.
fn finish_ready(app: &mut TestApp, key: &ThumbnailKey) {
    let bytes = jpeg(u32::from(key.size.width), u32::from(key.size.height) * 2);
    app.native_link_thumbnails
        .reconcile(std::slice::from_ref(key));
    let job = app
        .native_link_thumbnails
        .admit(key, || Some(bytes.clone()))
        .unwrap();
    let picker = Arc::new(Mutex::new(Picker::halfblocks()));
    let protocol = prepare_thumbnail(&bytes, key.size, &picker).unwrap();
    assert!(
        app.native_link_thumbnails
            .finish(job.ticket(), Some(protocol))
    );
}

fn changed_rect(before: &Buffer, after: &Buffer) -> Rect {
    let changed = before
        .area
        .rows()
        .flat_map(|row| row.columns())
        .filter(|&position| before[position] != after[position])
        .collect::<Vec<_>>();
    assert!(!changed.is_empty(), "ready thumbnail must paint cells");
    let left = changed.iter().map(|p| p.x).min().unwrap();
    let top = changed.iter().map(|p| p.y).min().unwrap();
    let right = changed.iter().map(|p| p.x).max().unwrap() + 1;
    let bottom = changed.iter().map(|p| p.y).max().unwrap() + 1;
    Rect::new(left, top, right - left, bottom - top)
}

// Exercise the actual clipped TempBuffer viewport, not a synthetic visible-key list.
fn viewport(app: &mut TestApp, area: Rect, bottom: isize) -> (Buffer, Vec<ThumbnailKey>) {
    app.mouse_hit_map.urls.clear();
    app.mouse_hit_map.messages.clear();
    let mut terminal = Terminal::new(TestBackend::new(180, 60)).unwrap();
    let mut plan = MediaRenderPlan::default();
    terminal
        .draw(|frame| {
            message_viewport::render(
                frame,
                app,
                area,
                &["native-ui".into()],
                &[AuthorGroupContext::STARTS_GROUP],
                0,
                area.width as isize,
                0,
                bottom,
                &mut MessageListCounts::default(),
                &mut VisibilityPlan::default(),
                MessageTextMode::Chat,
                &mut plan,
            );
        })
        .unwrap();
    (terminal.backend().buffer().clone(), keys(plan))
}

fn shortcut_popup_area(frame: Rect) -> Rect {
    // Exact inline geometry in ui::render_keybind_overlays; it does not use
    // centered_modal_layout or a percentage-based centered_rect helper.
    let width = 42.min(frame.width.saturating_sub(2));
    let height = (crate::keybindings::canonical_shortcuts().len() as u16 + 2)
        .min(frame.height.saturating_sub(2));
    Rect::new(
        frame.x + frame.width.saturating_sub(width) / 2,
        frame.y + frame.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

#[test]
fn visible_native_text_preserves_label_source_and_exact_destination_at_shipped_sizes() {
    for size in SIZES {
        let source = format!("Read {URL}");
        let mut app = fixture(&source, Vec::new());
        let (buffer, demand) = draw(&mut app, size);
        let painted = text(&buffer);
        assert!(painted.contains(TITLE), "native title at {size:?}");
        assert!(
            painted.contains(DESCRIPTION),
            "native description at {size:?}"
        );
        assert!(painted.contains(LABEL));
        assert!(demand.is_empty(), "text metadata is not image demand");
        source_and_hits(&app, &source);
        assert_eq!(hit_cells(&app, &buffer), LABEL);
        assert_eq!(app.native_link_thumbnails.outstanding(), 0);
        assert_eq!(app.native_link_thumbnails.len(), 0);
    }
}

#[test]
fn missing_metadata_keeps_old_label_and_metadata_markdown_stays_literal() {
    let mut plain = TestApp::new();
    let chat: wr::JID = "plain@example.test".to_owned().into();
    let mut item = message(&chat, "native-ui", 1);
    item.message = wr::MessageContent::Text(URL.into());
    plain.add_message(item);
    plain.open_chat = Some(chat);
    let (buffer, demand) = draw(&mut plain, (180, 50));
    assert!(text(&buffer).contains(LABEL));
    assert!(!text(&buffer).contains(TITLE));
    assert!(demand.is_empty());
    source_and_hits(&plain, URL);

    let mut app = fixture(URL, Vec::new());
    let item = app.messages["native-ui"].clone();
    app.restore_native_link_preview(crate::app::native_link_previews::NativeLinkPreviewRecord {
        chat: item.info.chat,
        message_id: item.info.id,
        source: URL.into(),
        preview: wr::NativeLinkPreview {
            matched_text: URL.into(),
            title: "*Native* [site](https://evil.test)".into(),
            description: "`literal` **description**".into(),
            jpeg_thumbnail: Vec::new(),
        },
    });
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 12));
    let item = app.messages["native-ui"].clone();
    let mut plan = MediaRenderPlan::default();
    render_message(
        &mut buffer,
        &item,
        false,
        AuthorGroupContext::STARTS_GROUP,
        &mut app,
        Rect::new(0, 0, 100, 12),
        &mut plan,
        false,
        MessageTextMode::Chat,
    );
    assert!(text(&buffer).contains("*Native* [site](https://evil.test)"));
    assert!(text(&buffer).contains("`literal` **description**"));
    source_and_hits(&app, URL);
    assert_eq!(hit_cells(&app, &buffer), LABEL);
    assert!(keys(plan).is_empty());
}
