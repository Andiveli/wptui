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
    test_support::{FixedPickerApp as TestApp, message},
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
    directional_fixture(source, thumbnail, false)
}

fn directional_fixture(source: &str, thumbnail: Vec<u8>, outgoing: bool) -> TestApp {
    let mut app = TestApp::new();
    let chat: wr::JID = "fixture@example.test".to_owned().into();
    let mut item = message(&chat, "native-ui", 1_704_110_400);
    item.message = wr::MessageContent::Text(source.into());
    item.info.is_from_me = outgoing;
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
fn symmetric_plain_and_chosen_markdown_labels_keep_native_card_and_ready_thumbnail() {
    for outgoing in [false, true] {
        for (source, label) in [
            (format!("Read {URL}"), LABEL),
            (format!("[My chosen label]({URL})"), "My chosen label"),
        ] {
            let mut app = directional_fixture(&source, jpeg(32, 16), outgoing);
            let (absent, key) = single_key(&mut app, (180, 50));
            assert!(text(&absent).contains(TITLE));
            assert!(text(&absent).contains(DESCRIPTION));
            assert_eq!(hit_cells(&app, &absent), label);
            source_and_hits(&app, &source);
            assert_eq!(app.native_link_thumbnails.outstanding(), 0);
            finish_ready(&mut app, &key);
            let (ready, refreshed) = single_key(&mut app, (180, 50));
            assert_eq!(refreshed, key);
            assert_eq!(changed_rect(&absent, &ready).as_size(), key.size);
            assert_eq!(hit_cells(&app, &ready), label);
            source_and_hits(&app, &source);
            assert_eq!(app.native_link_thumbnails.state(&key), EntryState::Ready);
            assert_eq!(app.native_link_thumbnails.outstanding(), 0);
        }
    }
}

#[test]
fn missing_or_invalid_metadata_keeps_symmetric_link_fallback() {
    for outgoing in [false, true] {
        for (source, label) in [
            (URL.to_owned(), LABEL),
            (format!("[My chosen label]({URL})"), "My chosen label"),
        ] {
            for invalid in [false, true] {
                let mut app = TestApp::new();
                let chat: wr::JID = "fallback@example.test".to_owned().into();
                let mut item = message(&chat, "native-ui", 1);
                item.info.is_from_me = outgoing;
                item.message = wr::MessageContent::Text(source.clone().into());
                if invalid {
                    wr::store_native_link_preview(
                        &item.info,
                        &source,
                        wr::NativeLinkPreview {
                            matched_text: URL.into(),
                            title: "bad\u{1b}title".into(),
                            description: DESCRIPTION.into(),
                            jpeg_thumbnail: jpeg(32, 16),
                        },
                    );
                }
                app.process_message(item, true);
                app.open_chat = Some(chat);
                let (buffer, demand) = draw(&mut app, (180, 50));
                assert!(text(&buffer).contains(label));
                assert!(!text(&buffer).contains(DESCRIPTION));
                assert_eq!(hit_cells(&app, &buffer), label);
                source_and_hits(&app, &source);
                assert!(app.native_link_preview(&"native-ui".into()).is_none());
                assert!(demand.is_empty());
                assert_eq!(app.native_link_thumbnails.len(), 0);
                assert_eq!(app.native_link_thumbnails.outstanding(), 0);
            }
        }
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
        "status",
        "caption",
        "system",
        "inline-code",
        "fence",
        "image",
        "brackets",
        "quote",
        "protected-duplicate",
        "protected-image-duplicate",
        "protected-explicit-duplicate",
    ] {
        let source = match case {
            "inline-code" => format!("`{URL}`"),
            "fence" => format!("```\n{URL}\n```"),
            "image" => format!("![image]({URL})"),
            "brackets" => format!("[{URL}]"),
            "protected-duplicate" => format!("```\n{URL}\n```\nthen {URL}"),
            "protected-image-duplicate" => format!("![image]({URL})\nthen {URL}"),
            "protected-explicit-duplicate" => format!("```\n{URL}\n```\nthen [Chosen]({URL})"),
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
        if matches!(
            case,
            "protected-duplicate" | "protected-image-duplicate" | "protected-explicit-duplicate"
        ) {
            let label = if case == "protected-explicit-duplicate" {
                "Chosen"
            } else {
                LABEL
            };
            assert!(
                painted.contains(label),
                "later occurrence retains its label"
            );
            source_and_hits(&app, &source);
            assert!(hit_cells(&app, &buffer).ends_with(label));
        } else if case == "quote" {
            assert!(painted.contains(URL), "quote snippet remains raw");
            assert!(
                app.mouse_hit_map.urls.is_empty(),
                "quote creates no URL hits"
            );
        } else if case == "system" {
            assert!(painted.contains(wr::VIEW_ONCE_UNAVAILABLE_DESCRIPTION));
        } else {
            assert!(painted.contains(URL), "{case}");
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
fn phone_observation_suppresses_only_intersecting_native_paint_and_demand() {
    use crate::app::phone_lookup_observation::ObservationView;

    let size = (180, 50);
    let source = format!("Read {URL}");
    let mut app = fixture(&source, jpeg(32, 16));
    let chat = app.messages["native-ui"].info.chat.clone();
    let mut outside = message(&chat, "outside-ui", 1_704_110_401);
    outside.message = wr::MessageContent::Text(source.clone().into());
    wr::store_native_link_preview(
        &outside.info,
        &source,
        wr::NativeLinkPreview {
            matched_text: URL.into(),
            title: TITLE.into(),
            description: DESCRIPTION.into(),
            jpeg_thumbnail: jpeg(32, 16),
        },
    );
    app.process_message(outside, true);
    app.message_list_state.reset();
    let (absent, demand) = draw(&mut app, size);
    assert_eq!(demand.len(), 2, "both full images initially eligible");
    let key_for = |id: &str| {
        demand
            .iter()
            .find(|key| key.message_id.as_ref() == id)
            .unwrap()
            .clone()
    };
    let covered = key_for("native-ui");
    let outside = key_for("outside-ui");
    let width = app.mouse_hit_map.message_list.unwrap().width as usize;
    let measured = height(&mut app, width);
    let anchor = app.message_list_state.viewport_anchor.clone();
    let hits = app
        .mouse_hit_map
        .urls
        .iter()
        .map(|hit| (hit.id.clone(), hit.area, hit.url.clone()))
        .collect::<Vec<_>>();
    let (range, metadata) = app.native_link_preview(&covered.message_id).unwrap();
    let title = metadata.title.clone();
    let description = metadata.description.clone();
    let open_observation = || {
        let observation = wr::LocalPhoneLookupObservation::from_display_text(
            "Requested: \"+15551234567\"\nIsIn: false".into(),
        )
        .unwrap();
        Some(ObservationView::new(observation))
    };

    // Draw only records work. Explicit Halfblocks preparation is outside draw;
    // FixedPickerApp supplies fake ports and never queries terminal capabilities.
    app.native_link_thumbnails.reconcile(&demand);
    let bytes = jpeg(
        u32::from(covered.size.width),
        u32::from(covered.size.height) * 2,
    );
    let covered_job = app
        .native_link_thumbnails
        .admit(&covered, || Some(bytes.clone()))
        .unwrap();
    let outside_job = app
        .native_link_thumbnails
        .admit(&outside, || Some(bytes.clone()))
        .unwrap();
    let (loading, loading_demand) = draw(&mut app, size);
    assert_eq!(loading, absent, "loading retains reserved geometry");
    assert_eq!(loading_demand, demand);
    assert_eq!(app.native_link_thumbnails.outstanding(), 2);
    app.phone_lookup_observation = open_observation();
    let (loading_overlay, loading_overlay_demand) = draw(&mut app, size);
    assert!(text(&loading_overlay).contains("Local number response (private)"));
    assert_eq!(height(&mut app, width), measured);
    assert_eq!(app.message_list_state.viewport_anchor, anchor);
    app.phone_lookup_observation = None;

    let picker = Arc::new(Mutex::new(Picker::halfblocks()));
    let covered_protocol = prepare_thumbnail(&bytes, covered.size, &picker).unwrap();
    assert!(
        app.native_link_thumbnails
            .finish(covered_job.ticket(), Some(covered_protocol))
    );
    let (covered_ready, refreshed) = draw(&mut app, size);
    assert_eq!(refreshed, demand);
    let covered_image = changed_rect(&loading, &covered_ready);
    assert_eq!(covered_image.as_size(), covered.size);
    let outside_protocol = prepare_thumbnail(&bytes, outside.size, &picker).unwrap();
    assert!(
        app.native_link_thumbnails
            .finish(outside_job.ticket(), Some(outside_protocol))
    );
    let (ready, refreshed) = draw(&mut app, size);
    assert_eq!(refreshed, demand);
    let outside_image = changed_rect(&covered_ready, &ready);
    assert_eq!(outside_image.as_size(), outside.size);

    // This is the existing public calculation used by the actual modal renderer,
    // not a new test-only geometry helper. Prove the fixture before testing guards.
    let modal = crate::ui::centered_modal_layout(ready.area);
    assert_eq!(modal, Rect::new(54, 17, 72, 16));
    let intersection = covered_image.intersection(modal);
    assert!(!intersection.is_empty(), "actual modal overlap required");
    assert_ne!(intersection, covered_image, "partial overlap required");
    assert!(outside_image.intersection(modal).is_empty());
    assert!(covered_image.intersection(outside_image).is_empty());
    for image in [covered_image, outside_image] {
        assert_eq!(
            image.intersection(app.mouse_hit_map.message_list.unwrap()),
            image
        );
        assert_eq!(image.intersection(ready.area), image);
    }
    // Successful full-image paint/demand above proves no other overlay, section,
    // or partial-viewport guard can account for suppression in this same frame.

    app.phone_lookup_observation = open_observation();
    let (overlaid, overlaid_demand) = draw(&mut app, size);
    assert!(text(&overlaid).contains("Local number response (private)"));
    assert_eq!(
        overlaid_demand,
        vec![outside.clone()],
        "phone observation must exclude intersecting READY image demand, not uncovered demand"
    );
    assert_eq!(
        loading_overlay_demand,
        vec![outside.clone()],
        "phone observation must also exclude intersecting loading image demand"
    );
    // Modal Clear overwrites intersecting cells even on the broken path. Only
    // compare cells OUTSIDE it: the uncovered part of a suppressed READY image
    // must remain the loading fallback, while the other image still paints.
    for y in covered_image.y..covered_image.bottom() {
        for x in covered_image.x..covered_image.right() {
            if Rect::new(x, y, 1, 1).intersection(modal).is_empty() {
                assert_ne!(ready[(x, y)], loading[(x, y)], "prepared cell proof");
                assert_eq!(overlaid[(x, y)], loading[(x, y)], "no partial READY paint");
            }
        }
    }
    for y in outside_image.y..outside_image.bottom() {
        for x in outside_image.x..outside_image.right() {
            assert_ne!(ready[(x, y)], loading[(x, y)], "prepared cell proof");
            assert_eq!(overlaid[(x, y)], ready[(x, y)], "outside READY paint");
        }
    }
    assert_eq!(height(&mut app, width), measured);
    assert_eq!(app.message_list_state.viewport_anchor, anchor);
    assert_eq!(
        app.mouse_hit_map
            .urls
            .iter()
            .map(|hit| { (hit.id.clone(), hit.area, hit.url.clone()) })
            .collect::<Vec<_>>(),
        hits,
        "overlay input guards preserve underlying hits"
    );
    assert!(hits.iter().all(|(_, _, url)| url == URL));
    for key in [&covered, &outside] {
        assert!(hits.iter().any(|(id, _, _)| id == &key.message_id));
    }
    assert_eq!(app.native_link_thumbnails.outstanding(), 0);
    assert_eq!(app.native_link_thumbnails.len(), 2);
    for key in [&covered, &outside] {
        assert_eq!(app.native_link_thumbnails.state(key), EntryState::Ready);
    }

    app.phone_lookup_observation = None;
    let (closed, restored) = draw(&mut app, size);
    assert_eq!(closed, ready, "closing restores paint");
    assert_eq!(restored, demand);
    assert_eq!(height(&mut app, width), measured);
    assert_eq!(app.message_list_state.viewport_anchor, anchor);
    for key in [&covered, &outside] {
        let wr::MessageContent::Text(stored) = &app.messages[&key.message_id].message else {
            panic!("text")
        };
        assert!(Arc::ptr_eq(stored, &key.source));
        assert_eq!(stored.as_ref(), source);
        assert_eq!(key.matched_url.as_ref(), URL);
    }
    let (restored_range, metadata) = app.native_link_preview(&covered.message_id).unwrap();
    assert_eq!(restored_range, range);
    assert!(Arc::ptr_eq(&metadata.title, &title));
    assert!(Arc::ptr_eq(&metadata.description, &description));
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
fn document_heights_remain_non_native_and_outgoing_fallback_keeps_exact_destination() {
    let mut app = fixture(URL, Vec::new());
    let mut item = app.messages["native-ui"].clone();
    item.info.is_from_me = true;
    item.message = wr::MessageContent::File(wr::FileContent {
        path: "document.txt".into(),
        kind: wr::FileKind::Document,
        caption: Some(URL.into()),
        ..Default::default()
    });
    for width in [20, 80, 140] {
        let expected = inline_content_lines(URL, &[], None, width).len() + 2;
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
            "other.test · kept"
        );
        assert!(matches!(&app.messages[&other.info.id].message,
            wr::MessageContent::Text(stored) if stored.as_ref() == other_url));
    }
}
