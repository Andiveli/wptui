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
    test_support::{
        FakeAvatarQuery, FakeChatReadCursorPort, FakeChatSettingsQuery, FakeCommunityQuery,
        FakeContactSource, FakeDmResolver, FakeMessagePushNamePort, FixedClock,
        RecordingChatReadSyncPort, RecordingNotifier, message,
    },
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

// Keep disposable state alive until the explicitly configured App is dropped.
// Unlike the general TestApp constructor, this never queries terminal capabilities.
struct TestApp {
    app: App<'static>,
    _dir: tempfile::TempDir,
}

impl TestApp {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let picker = Picker::halfblocks();
        let protocol = picker.protocol_type();
        let mut app = crate::app::bootstrap::with_data_dir_and_picker_and_ports(
            dir.path(),
            dir.path(),
            picker,
            protocol,
            Box::new(FixedClock::new(1_704_110_400)),
            Box::new(RecordingNotifier::default()),
        );
        app.set_contact_source(Box::new(FakeContactSource::default()));
        app.set_chat_settings_query(Box::new(FakeChatSettingsQuery::default()));
        app.set_message_push_name(Box::new(FakeMessagePushNamePort::default()));
        app.set_community_query(Box::new(FakeCommunityQuery::default()));
        app.set_dm_resolver(Box::new(FakeDmResolver::default()));
        app.set_avatar_query(Arc::new(FakeAvatarQuery::default()));
        app.set_chat_read_sync(Box::new(RecordingChatReadSyncPort::default()));
        app.set_chat_read_cursor(Box::new(FakeChatReadCursorPort::default()));
        app.db_handler.init();
        Self { app, _dir: dir }
    }
}

impl std::ops::Deref for TestApp {
    type Target = App<'static>;

    fn deref(&self) -> &Self::Target {
        &self.app
    }
}

impl std::ops::DerefMut for TestApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.app
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        self.app.shutdown_avatar_runtime();
        self.app.shutdown_read_receipt_worker();
        self.app.shutdown_read_sync_worker();
        self.app.db_handler.stop();
    }
}

fn fixture(source: &str, thumbnail: Vec<u8>) -> TestApp {
    let mut app = TestApp::new();
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

#[test]
fn excluded_modes_and_protected_source_occurrences_never_show_native_cards() {
    for case in [
        "outgoing",
        "status",
        "caption",
        "system",
        "inline-code",
        "fence",
        "explicit",
        "image",
        "brackets",
        "quote",
        "protected-duplicate",
    ] {
        let source = match case {
            "inline-code" => format!("`{URL}`"),
            "fence" => format!("```\n{URL}\n```"),
            "explicit" => format!("[site]({URL})"),
            "image" => format!("![image]({URL})"),
            "brackets" => format!("[{URL}]"),
            "protected-duplicate" => format!("```\n{URL}\n```\nthen {URL}"),
            _ => URL.to_owned(),
        };
        let mut app = fixture(&source, jpeg(32, 16));
        if case == "inline-code" {
            assert!(
                app.native_link_preview(&"native-ui".into()).is_none(),
                "this inline-code fixture has no exact native URL binding"
            );
        }
        if matches!(case, "fence" | "protected-duplicate") {
            let (range, _) = app.native_link_preview(&"native-ui".into()).unwrap();
            assert_eq!(
                range.start,
                source.find(URL).unwrap(),
                "T1 bound occurrence"
            );
        }
        let mut item = app.messages["native-ui"].clone();
        let mut mode = MessageTextMode::Chat;
        match case {
            "outgoing" => item.info.is_from_me = true,
            "status" => mode = MessageTextMode::Status,
            "caption" => {
                item.message = wr::MessageContent::File(wr::FileContent {
                    path: "document.txt".into(),
                    kind: wr::FileKind::Document,
                    caption: Some(URL.into()),
                    ..Default::default()
                })
            }
            "system" => item.message = wr::MessageContent::ViewOnceUnavailable,
            "quote" => {
                let mut reply = message(&item.info.chat, "reply-ui", 2);
                reply.info.quote_id = Some(item.info.id.clone());
                reply.message = wr::MessageContent::Text("Reply body".into());
                item = reply;
            }
            _ => {}
        }
        let mut buffer = Buffer::empty(Rect::new(0, 0, 140, 16));
        let mut plan = MediaRenderPlan::default();
        render_message(
            &mut buffer,
            &item,
            false,
            AuthorGroupContext::STARTS_GROUP,
            &mut app,
            Rect::new(0, 0, 140, 16),
            &mut plan,
            true,
            mode,
        );
        let painted = text(&buffer);
        assert!(!painted.contains(TITLE), "{case}");
        assert!(!painted.contains(DESCRIPTION), "{case}");
        assert!(keys(plan).is_empty(), "{case}");
        if case == "protected-duplicate" {
            assert!(
                painted.contains(LABEL),
                "later plain occurrence retains label"
            );
            source_and_hits(&app, &source);
            assert!(hit_cells(&app, &buffer).ends_with(LABEL));
        } else if case == "quote" {
            assert!(painted.contains(URL), "quote snippet remains raw");
            assert!(
                app.mouse_hit_map.urls.is_empty(),
                "quote creates no URL hits"
            );
        } else if case == "system" {
            assert!(painted.contains(wr::VIEW_ONCE_UNAVAILABLE_DESCRIPTION));
        } else {
            assert!(
                painted.contains(if case == "explicit" { "site" } else { URL }),
                "{case}"
            );
            assert!(app.mouse_hit_map.urls.iter().all(|hit| hit.url == URL));
        }
        assert!(matches!(&app.messages["native-ui"].message,
            wr::MessageContent::Text(stored) if stored.as_ref() == source));
    }
}

#[test]
fn composer_and_hidden_conversations_produce_no_native_work_or_hidden_hits() {
    use crate::app::actions::Section;

    for hidden in ["other-chat", "news", "logout", "composer-only"] {
        let mut app = fixture(URL, jpeg(32, 16));
        match hidden {
            "other-chat" | "composer-only" => {
                let chat: wr::JID = "other@example.test".to_owned().into();
                let mut other = message(&chat, "other-chat-body", 1_704_110_401);
                other.message = wr::MessageContent::Text("Other conversation".into());
                app.add_message(other);
                app.open_chat = Some(chat);
                if hidden == "composer-only" {
                    app.composer.insert_text(URL);
                }
            }
            "news" => app.selected_section = Section::News,
            "logout" => app.rail_on_logout = true,
            _ => unreachable!(),
        }
        let (buffer, demand) = draw(&mut app, (180, 50));
        assert!(demand.is_empty(), "{hidden}");
        assert!(app.mouse_hit_map.urls.is_empty(), "hidden hits: {hidden}");
        assert!(!text(&buffer).contains(TITLE), "{hidden}");
        if matches!(hidden, "other-chat" | "composer-only") {
            assert!(text(&buffer).contains("Other conversation"));
        }
        assert_eq!(app.native_link_thumbnails.outstanding(), 0);
        if hidden == "composer-only" {
            assert_eq!(app.composer.text(), URL);
            assert!(text(&buffer).contains(URL));
        }
    }
}

#[test]
fn shortcut_popup_preserves_hits_and_only_suppresses_occluded_native_images() {
    for size in SIZES {
        let mut app = fixture(URL, jpeg(32, 16));
        let (absent, key) = single_key(&mut app, size);
        finish_ready(&mut app, &key);
        let (ready, refreshed) = single_key(&mut app, size);
        assert_eq!(refreshed, key);
        let image = changed_rect(&absent, &ready);
        assert_eq!(image.as_size(), key.size);
        let popup = shortcut_popup_area(ready.area);
        let occluded = !image.intersection(popup).is_empty();
        let hits = app
            .mouse_hit_map
            .urls
            .iter()
            .map(|hit| (hit.area, hit.url.clone()))
            .collect::<Vec<_>>();
        assert!(
            !hits.is_empty(),
            "retain original hit regions, not modal cleanup"
        );
        app.shortcut_popup = true;
        for _ in 0..2 {
            let (overlaid, demand) = draw(&mut app, size);
            assert_eq!(
                app.mouse_hit_map
                    .urls
                    .iter()
                    .map(|hit| (hit.area, hit.url.clone()))
                    .collect::<Vec<_>>(),
                hits
            );
            if occluded {
                assert!(
                    demand.is_empty(),
                    "entire native image must be safely paintable"
                );
            } else {
                assert_eq!(
                    demand,
                    vec![key.clone()],
                    "uncovered image remains visible demand"
                );
            }
            for y in image.y..image.bottom() {
                for x in image.x..image.right() {
                    if Rect::new(x, y, 1, 1).intersection(popup).is_empty() {
                        let expected = if occluded {
                            &absent[(x, y)]
                        } else {
                            &ready[(x, y)]
                        };
                        assert_eq!(
                            &overlaid[(x, y)],
                            expected,
                            "native cell outside popup at {size:?} ({x},{y})"
                        );
                    }
                }
            }
            assert_eq!(app.native_link_thumbnails.state(&key), EntryState::Ready);
            assert_eq!(app.native_link_thumbnails.outstanding(), 0);
        }
    }
}

#[test]
fn visible_unselected_requests_but_selected_offscreen_does_not() {
    for size in SIZES {
        let mut app = fixture(URL, jpeg(32, 16));
        assert!(app.message_list_state.selected.is_none());
        let (_, key) = single_key(&mut app, size);
        assert_eq!(app.native_link_thumbnails.state(&key), EntryState::Absent);
        assert_eq!(
            app.native_link_thumbnails.outstanding(),
            0,
            "draw only records demand"
        );
        let chat = app.messages["native-ui"].info.chat.clone();
        for index in 0..60 {
            let mut spacer = message(&chat, &format!("newer-{index}"), 1_704_110_401 + index);
            spacer.message = wr::MessageContent::Text("Newer plain message".into());
            app.add_message(spacer);
        }
        let (items, _, _) = chat_message_sequence(&mut app, &chat).unwrap();
        let native_index = items
            .iter()
            .position(|id| id.as_ref() == "native-ui")
            .unwrap();
        assert_eq!(
            native_index, 60,
            "native message is oldest, not newest index zero"
        );
        app.message_list_state.selected = Some(native_index);
        app.message_list_state.selected_message = Some("native-ui".into());
        app.message_list_state.update_selected = false;
        // Keep normal latest-history offset zero. The previous anchor refers to
        // native-ui at index zero; reconciliation rejects it after these inserts.
        assert_eq!(app.message_list_state.offset, 0);
        let (buffer, demand) = draw(&mut app, size);
        assert!(text(&buffer).contains("Newer plain message"));
        assert!(demand.is_empty(), "selected but offscreen at {size:?}");
        assert!(app.mouse_hit_map.urls.is_empty());
        assert!(
            !app.mouse_hit_map
                .messages
                .iter()
                .any(|hit| hit.id.as_ref() == "native-ui")
        );
        assert_eq!(app.message_list_state.selected, Some(native_index));
        assert_eq!(
            app.message_list_state.get_selected_message().as_deref(),
            Some("native-ui")
        );
        assert_eq!(app.native_link_thumbnails.outstanding(), 0);
    }
}

#[test]
fn repeated_loading_ready_and_failed_draws_refresh_same_key_without_geometry_or_hit_changes() {
    for size in SIZES {
        for ready in [true, false] {
            let mut app = fixture(URL, jpeg(32, 16));
            let (absent, key) = single_key(&mut app, size);
            let width = app.mouse_hit_map.message_list.unwrap().width as usize;
            let measured = height(&mut app, width);
            let anchor = app.message_list_state.viewport_anchor.clone();
            let selection = app.message_list_state.get_selected_message();
            let hits = app
                .mouse_hit_map
                .urls
                .iter()
                .map(|hit| (hit.area, hit.url.clone()))
                .collect::<Vec<_>>();
            app.native_link_thumbnails
                .reconcile(std::slice::from_ref(&key));
            let bytes = jpeg(u32::from(key.size.width), u32::from(key.size.height) * 2);
            let job = app
                .native_link_thumbnails
                .admit(&key, || Some(bytes.clone()))
                .unwrap();
            for _ in 0..2 {
                let (loading, refreshed) = single_key(&mut app, size);
                assert_eq!(refreshed, key);
                assert_eq!(loading, absent);
                assert_eq!(app.native_link_thumbnails.outstanding(), 1);
            }
            let protocol = if ready {
                prepare_thumbnail(
                    &bytes,
                    key.size,
                    &Arc::new(Mutex::new(Picker::halfblocks())),
                )
            } else {
                None
            };
            assert!(app.native_link_thumbnails.finish(job.ticket(), protocol));
            for _ in 0..2 {
                let (painted, refreshed) = single_key(&mut app, size);
                assert_eq!(refreshed, key);
                assert_eq!(height(&mut app, width), measured);
                assert_eq!(app.message_list_state.viewport_anchor, anchor);
                assert_eq!(app.message_list_state.get_selected_message(), selection);
                assert_eq!(
                    app.mouse_hit_map
                        .urls
                        .iter()
                        .map(|hit| (hit.area, hit.url.clone()))
                        .collect::<Vec<_>>(),
                    hits
                );
                assert_eq!(app.native_link_thumbnails.outstanding(), 0);
                assert_eq!(app.native_link_thumbnails.len(), 1);
                assert!(
                    app.native_link_thumbnails
                        .admit(&key, || panic!("draw retried cached bytes"))
                        .is_none()
                );
                assert_eq!(
                    app.native_link_thumbnails.state(&key),
                    if ready {
                        EntryState::Ready
                    } else {
                        EntryState::Failed
                    }
                );
                if ready {
                    let image = changed_rect(&absent, &painted);
                    assert_eq!(image.as_size(), key.size, "full reserved geometry");
                    assert_eq!(
                        image.intersection(app.mouse_hit_map.message_list.unwrap()),
                        image
                    );
                } else {
                    assert_eq!(
                        painted, absent,
                        "negative cache keeps reserved text fallback"
                    );
                }
                source_and_hits(&app, URL);
            }
        }
    }
}

#[test]
fn clipped_thumbnail_requires_entire_reserved_rect_and_narrow_columns_stay_text_only() {
    let mut app = fixture(URL, jpeg(32, 16));
    let area = Rect::new(5, 5, 100, 40);
    let measured = height(&mut app, area.width as usize);
    let bottom = isize::try_from(area.y).expect("fixture row fits isize") + measured as isize;
    let (absent, demand) = viewport(&mut app, area, bottom);
    assert_eq!(
        demand.len(),
        1,
        "fully visible viewport must request thumbnail"
    );
    let key = demand[0].clone();
    finish_ready(&mut app, &key);
    let (ready, _) = viewport(&mut app, area, bottom);
    let image = changed_rect(&absent, &ready);
    assert_eq!(image.as_size(), key.size);
    for (name, clipped) in [
        (
            "one-image-row-clipped-at-top",
            Rect::new(area.x, image.y + 1, area.width, area.bottom() - image.y - 1),
        ),
        (
            "one-image-row-clipped-at-bottom",
            Rect::new(area.x, area.y, area.width, image.bottom() - area.y - 1),
        ),
        (
            "text-visible-image-offscreen",
            Rect::new(area.x, area.y, area.width, image.y - area.y),
        ),
    ] {
        let (painted, demand) = viewport(&mut app, clipped, bottom);
        assert!(demand.is_empty(), "{name}");
        assert!(
            !painted
                .content
                .iter()
                .any(|cell| matches!(cell.bg, Color::Rgb(r, _, _) if r > 200)),
            "no partial protocol: {name}"
        );
        assert!(
            app.mouse_hit_map
                .urls
                .iter()
                .all(|hit| hit.area.intersection(clipped) == hit.area)
        );
        assert_eq!(app.native_link_thumbnails.outstanding(), 0);
    }
    let tiny = Rect::new(5, 5, 1, 40);
    let tiny_height = height(&mut app, 1);
    let (_, demand) = viewport(&mut app, tiny, 5 + tiny_height as isize);
    assert!(
        demand.is_empty(),
        "no unsafe image in one-column text layout"
    );
}

#[test]
fn stale_source_or_size_protocol_is_not_painted_and_resize_replans_normal_draw() {
    for change in ["source", "size"] {
        let mut app = fixture(URL, jpeg(32, 16));
        let (absent, key) = single_key(&mut app, (180, 50));
        let mut stale = key.clone();
        if change == "source" {
            stale.source = format!("old {URL}").into();
        } else {
            stale.size.width += 1;
        }
        finish_ready(&mut app, &stale);
        let (painted, current) = single_key(&mut app, (180, 50));
        assert_eq!(current, key);
        assert_eq!(painted, absent, "stale {change} protocol must not paint");
        let narrow_width = key.size.width.saturating_sub(1).max(1);
        let area = Rect::new(5, 5, narrow_width, 40);
        let measured = height(&mut app, narrow_width as usize);
        let (_, demand) = viewport(&mut app, area, 5 + measured as isize);
        assert!(demand.iter().all(|new| new.source == key.source
            && new.matched_url == key.matched_url
            && new.size != stale.size
            && new.size.width <= narrow_width));
        let (_, restored) = single_key(&mut app, (180, 50));
        assert_eq!(restored, key);
    }
    let source = format!("{} {URL}", "words ".repeat(8));
    let mut app = fixture(&source, jpeg(32, 16));
    let (_, wide_key) = single_key(&mut app, (180, 50));
    let wide_width = app.mouse_hit_map.message_list.unwrap().width as usize;
    let wide_height = height(&mut app, wide_width);
    finish_ready(&mut app, &wide_key);
    let (_, narrow_key) = single_key(&mut app, (80, 24));
    let narrow_width = app.mouse_hit_map.message_list.unwrap().width as usize;
    assert!(narrow_width < wide_width);
    assert!(
        height(&mut app, narrow_width) > wide_height,
        "resize remeasures wrapped body"
    );
    assert!(usize::from(narrow_key.size.width) <= narrow_width);
    assert_eq!(narrow_key.source, wide_key.source);
    assert_eq!(narrow_key.matched_url, wide_key.matched_url);
    if narrow_key.size != wide_key.size {
        assert_eq!(
            app.native_link_thumbnails.state(&narrow_key),
            EntryState::Absent
        );
    }
    source_and_hits(&app, &source);
}

#[test]
fn invalid_jpeg_negative_cache_retains_native_text_and_original_input() {
    let mut app = fixture(URL, b"not JPEG".to_vec());
    let (before, key) = single_key(&mut app, (180, 50));
    assert!(text(&before).contains(TITLE));
    app.native_link_thumbnails
        .reconcile(std::slice::from_ref(&key));
    let job = app
        .native_link_thumbnails
        .admit(&key, || Some(b"not JPEG".to_vec()))
        .unwrap();
    let protocol = prepare_thumbnail(
        b"not JPEG",
        key.size,
        &Arc::new(Mutex::new(Picker::halfblocks())),
    );
    assert!(protocol.is_none());
    assert!(app.native_link_thumbnails.finish(job.ticket(), protocol));
    let (after, refreshed) = single_key(&mut app, (180, 50));
    assert_eq!(refreshed, key);
    assert_eq!(before, after);
    assert_eq!(app.native_link_thumbnails.state(&key), EntryState::Failed);
    assert_eq!(app.native_link_thumbnails.outstanding(), 0);
    assert!(
        app.native_link_thumbnails
            .admit(&key, || panic!("negative retry"))
            .is_none()
    );
    source_and_hits(&app, URL);
}

#[test]
fn outgoing_and_document_heights_remain_the_existing_non_native_contract() {
    let mut app = fixture(URL, Vec::new());
    for caption in [false, true] {
        let mut item = app.messages["native-ui"].clone();
        item.info.is_from_me = true;
        if caption {
            item.message = wr::MessageContent::File(wr::FileContent {
                path: "document.txt".into(),
                kind: wr::FileKind::Document,
                caption: Some(URL.into()),
                ..Default::default()
            });
        }
        for width in [20, 80, 140] {
            let expected =
                inline_content_lines(URL, &[], None, width).len() + if caption { 2 } else { 1 };
            assert_eq!(
                message_height(
                    &item,
                    width,
                    false,
                    AuthorGroupContext::STARTS_GROUP,
                    &mut app
                ),
                expected
            );
        }
    }
    let other_url = "https://other.test/kept?q=%2F#part";
    let chat = app.messages["native-ui"].info.chat.clone();
    let mut other = message(&chat, "other-layout", 1_704_110_401);
    other.info.is_from_me = true;
    other.message = wr::MessageContent::Text(other_url.into());
    app.add_message(other.clone());
    let expected = message_height(
        &other,
        100,
        false,
        AuthorGroupContext::STARTS_GROUP,
        &mut app,
    );
    for _ in 0..2 {
        let (buffer, _) = draw(&mut app, (180, 50));
        assert_eq!(
            message_height(
                &other,
                100,
                false,
                AuthorGroupContext::STARTS_GROUP,
                &mut app
            ),
            expected
        );
        let hits = app
            .mouse_hit_map
            .urls
            .iter()
            .filter(|hit| hit.id == other.info.id)
            .collect::<Vec<_>>();
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|hit| hit.url == other_url));
        assert_eq!(
            hits.iter()
                .map(|hit| buffer[(hit.area.x, hit.area.y)].symbol())
                .collect::<String>(),
            other_url
        );
        assert!(matches!(&app.messages[&other.info.id].message,
            wr::MessageContent::Text(stored) if stored.as_ref() == other_url));
    }
}
