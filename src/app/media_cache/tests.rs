use std::collections::VecDeque;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use ratatui::layout::Size;
use ratatui_image::picker::Picker;

use super::super::media_jobs::MediaJobOwner;
use super::super::test_support::TestApp;
use super::super::{AppEvent, AppInput, FileMeta, Metadata};
use whatsrust as wr;

fn file_message(id: &str, path: &str) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: id.into(),
            chat: wr::JID::from("chat@example.test".to_owned()),
            sender: wr::JID::from("sender@example.test".to_owned()),
            mentions_self: false,
            timestamp: 1,
            forwarding: Default::default(),
            is_from_me: false,
            quote_id: None,
            read_by: 0,
        },
        message: wr::MessageContent::File(wr::FileContent {
            kind: wr::FileKind::Image,
            path: path.into(),
            ..Default::default()
        }),
    }
}

#[test]
fn evicted_loaded_preview_becomes_reloadable() {
    let mut app = TestApp::new();
    let message = file_message("preview", "old.png");
    app.messages.insert(message.info.id.clone(), message);
    app.metadata
        .insert("preview".into(), Metadata::File(FileMeta::Loaded));
    app.mark_evicted_preview_reloadable(&Arc::from("old.png"));

    assert!(matches!(
        app.metadata.get(&wr::MessageId::from("preview")),
        Some(Metadata::File(FileMeta::Downloaded))
    ));
    assert!(
        !app.message_height_cache
            .contains(&wr::MessageId::from("preview"))
    );
}

#[test]
fn late_preview_result_requires_loading_and_same_path() {
    let mut app = TestApp::new();
    app.messages
        .insert("preview".into(), file_message("preview", "old.png"));
    let path = Arc::<str>::from("old.png");
    assert!(!app.preview_result_is_current(&"preview".into(), &path));
    app.metadata
        .insert("preview".into(), Metadata::File(FileMeta::Loading));
    assert!(app.preview_result_is_current(&"preview".into(), &path));
    assert!(!app.preview_result_is_current(&"preview".into(), &Arc::from("new.png")));
    app.metadata
        .insert("preview".into(), Metadata::File(FileMeta::Downloaded));
    assert!(!app.preview_result_is_current(&"preview".into(), &path));
}

#[test]
fn only_visible_stickers_schedule_playback() {
    let mut app = TestApp::new();
    let path = Arc::<str>::from("stickers/animated.webp");
    let animation = super::super::sticker_animation::decode_animated_webp(include_bytes!(
        "../../../tests/fixtures/sticker-animated.webp"
    ))
    .unwrap()
    .unwrap();
    app.sticker_playback.insert(
        path.clone(),
        super::super::sticker_animation::StickerPlayback::prepare(
            animation,
            &mut Picker::halfblocks(),
            Size::new(2, 2),
        ),
    );
    let start = Instant::now();
    assert!(!app.advance_visible_stickers(start));
    assert_eq!(app.sticker_deadline(), None);
    app.visible_stickers.insert(path.clone());
    assert!(!app.advance_visible_stickers(start));
    assert_eq!(
        app.sticker_deadline(),
        Some(start + Duration::from_millis(80))
    );
    app.attachment_viewer =
        Some(super::super::events::AttachmentViewerState::from_attachments(Vec::new(), 0));
    assert_eq!(app.sticker_deadline(), None);
    assert!(!app.advance_visible_stickers(start + Duration::from_millis(80)));
    assert_eq!(app.sticker_playback[&path].current, 0);
    app.attachment_viewer = None;
    assert!(!app.advance_visible_stickers(start + Duration::from_secs(1)));
    assert!(
        app.advance_visible_stickers(start + Duration::from_secs(1) + Duration::from_millis(80))
    );
    app.visible_stickers.clear();
    assert!(!app.advance_visible_stickers(start + Duration::from_millis(81)));
    assert_eq!(app.sticker_deadline(), None);
}

#[test]
fn fifth_visible_animation_keeps_first_frame_without_evicting_playback() {
    let mut app = TestApp::new();
    let mut paths = Vec::new();
    for index in 0..5 {
        let id = format!("sticker-{index}");
        let path = format!("sticker-{index}.webp");
        app.messages
            .insert(id.clone().into(), file_message(&id, &path));
        app.metadata
            .insert(id.clone().into(), Metadata::File(FileMeta::Loading));
        let animation = super::super::sticker_animation::decode_animated_webp(include_bytes!(
            "../../../tests/fixtures/sticker-animated.webp"
        ))
        .unwrap()
        .unwrap();
        let playback = super::super::sticker_animation::StickerPlayback::prepare(
            animation,
            &mut Picker::halfblocks(),
            Size::new(2, 2),
        );
        let path: Arc<str> = path.into();
        app.cache_sticker_preview(id.into(), path.clone(), playback);
        paths.push(path);
    }
    assert_eq!(app.sticker_playback.len(), 4);
    assert_eq!(app.image_cache.len(), 1);
    assert!(app.image_cache.contains_key(&paths[4]));
    for path in &paths {
        app.visible_stickers.insert(path.clone());
        app.touch_sticker_cache(path);
        assert!(matches!(
            app.metadata
                .get(&wr::MessageId::from(path.replace(".webp", ""))),
            Some(Metadata::File(FileMeta::Loaded))
        ));
    }
    assert_eq!(app.sticker_playback.len(), 4);
    assert_eq!(app.image_cache.len(), 1);
}

#[test]
fn simultaneous_sticker_loads_eventually_animate_after_worker_slots_free() {
    let mut app = TestApp::new();
    let mut jobs = MediaJobOwner::new();
    let (download_tx, _download_rx) = mpsc::channel();
    std::fs::create_dir_all(app.media_path.join("stickers")).unwrap();
    let mut paths = Vec::new();
    for index in 0..3 {
        let id = format!("animated-{index}");
        let path = format!("stickers/{id}.webp");
        std::fs::write(
            app.media_path.join(&path),
            include_bytes!("../../../tests/fixtures/sticker-animated.webp"),
        )
        .unwrap();
        let mut message = file_message(&id, &path);
        if let wr::MessageContent::File(file) = &mut message.message {
            file.kind = wr::FileKind::Sticker;
        }
        app.messages.insert(id.clone().into(), message);
        app.handle_media_viewer_event(
            AppEvent::LoadFilePreview(id.into()),
            &download_tx,
            &mut jobs,
        );
        paths.push(Arc::<str>::from(path));
    }
    for _ in 0..3 {
        let AppInput::App(event) = app.rx.recv_timeout(Duration::from_secs(5)).unwrap() else {
            panic!("expected sticker preview event");
        };
        app.handle_media_viewer_event(event, &download_tx, &mut jobs);
    }
    jobs.shutdown();
    for path in paths {
        assert!(
            app.sticker_playback.contains_key(&path),
            "{path} remained a still preview"
        );
    }
}

#[test]
fn touching_preview_moves_it_to_the_lru_tail() {
    let mut app = TestApp::new();
    let first = Arc::<str>::from("first.png");
    let second = Arc::<str>::from("second.png");
    app.image_cache_order.push_back(first.clone());
    app.image_cache_order.push_back(second.clone());
    super::touch_order(&mut app.image_cache_order, &first);

    assert_eq!(app.image_cache_order, VecDeque::from([second, first]));
}
