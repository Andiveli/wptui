use super::*;
use crate::app::actions::{ComposerAction, ConversationMode, FocusPane};
use crate::app::test_support::TestApp;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

#[test]
fn images_videos_and_pasted_images_are_eligible_but_documents_are_not() {
    let mut app = TestApp::new();
    let directory = tempfile::tempdir().unwrap();
    for (path, kind) in [
        ("photo.png", wr::FileKind::Image),
        ("clip.mp4", wr::FileKind::Video),
        ("notes.pdf", wr::FileKind::Document),
    ] {
        app.composer.queue_attachment(path.into(), kind);
    }
    crate::app::composer_input_paste::apply_clipboard_paste(
        &mut app.composer,
        directory.path(),
        Ok(crate::clipboard::ClipboardPaste::Png(vec![1, 2, 3])),
    )
    .unwrap();
    let requests = app.reconcile_composer_previews();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].attachment.display_name(), "photo.png");
    assert_eq!(requests[1].attachment.display_name(), "clip.mp4");
    assert_eq!(requests[2].attachment.kind, wr::FileKind::Image);
    assert_eq!(requests[2].attachment.path, app.composer.pending[3].path);
    assert!(app.reconcile_composer_previews().is_empty());
}

#[test]
fn removed_replaced_sent_and_chat_switched_previews_reject_old_results() {
    for change in ["remove", "replace", "send", "chat", "section", "clear"] {
        let mut app = TestApp::new();
        app.composer
            .queue_attachment("photo.png".into(), wr::FileKind::Image);
        let old = app.reconcile_composer_previews().remove(0);
        match change {
            "remove" => {
                app.composer.apply(ComposerAction::RemoveLastAttachment);
            }
            "replace" => {
                app.composer.pending.clear();
                app.composer
                    .queue_attachment("photo.png".into(), wr::FileKind::Image);
            }
            "send" => {
                app.composer.apply(ComposerAction::Submit);
            }
            "chat" => {
                app.open_chat = Some("other@s.whatsapp.net".to_owned().into());
            }
            "section" => app.selected_section = Section::Status,
            _ => app.composer.set_blocked(true),
        }
        // Supply a successful result, not just failure, after invalidation.
        let protocol = app
            .picker
            .lock()
            .unwrap()
            .new_resize_protocol(image::DynamicImage::new_rgb8(8, 8));
        assert!(
            !app.finish_composer_previews(vec![(old.generation, Some(protocol))]),
            "{change}"
        );
        assert!(
            !app.composer_previews
                .entries
                .iter()
                .any(|entry| entry.generation == old.generation)
        );
    }
}

#[test]
fn preview_work_is_bounded_even_when_attachments_are_replaced_while_loading() {
    let mut app = TestApp::new();
    for _ in 0..20 {
        app.composer
            .queue_attachment("photo.png".into(), wr::FileKind::Image);
    }
    let requests = app.reconcile_composer_previews();
    assert_eq!(requests.len(), MAX_PREVIEWS);
    app.composer.pending.clear();
    app.composer
        .queue_attachment("new.png".into(), wr::FileKind::Image);
    assert!(app.reconcile_composer_previews().is_empty());
    app.finish_composer_previews(requests.into_iter().map(|r| (r.generation, None)).collect());
    assert_eq!(app.reconcile_composer_previews().len(), 1);
}

#[test]
fn ready_and_failed_results_are_cached_then_removed_on_send() {
    let mut app = TestApp::new();
    app.composer
        .queue_attachment("photo.png".into(), wr::FileKind::Image);
    app.composer
        .queue_attachment("clip.mp4".into(), wr::FileKind::Video);
    let requests = app.reconcile_composer_previews();
    let protocol = app
        .picker
        .lock()
        .unwrap()
        .new_resize_protocol(image::DynamicImage::new_rgb8(8, 8));
    assert!(app.finish_composer_previews(vec![
        (requests[0].generation, Some(protocol)),
        (requests[1].generation, None),
    ]));
    assert!(app.reconcile_composer_previews().is_empty());
    let attachment = app.composer.pending[0].clone();
    assert!(app.composer_previews.protocol_mut(&attachment).is_some());
    app.composer.apply(ComposerAction::Submit);
    assert!(app.reconcile_composer_previews().is_empty());
    assert!(app.composer_previews.protocol_mut(&attachment).is_none());
    assert!(app.composer_previews.entries.is_empty());
}

#[test]
fn tiny_composer_keeps_text_and_large_composer_keeps_fallback_labels_without_io() {
    for (width, height) in [(20, 7), (60, 20)] {
        let mut app = TestApp::new();
        app.open_chat = Some("alice@s.whatsapp.net".to_owned().into());
        app.focus_pane = FocusPane::Conversation;
        app.conversation_mode = ConversationMode::ComposerEditing;
        app.composer.insert_text("caption");
        app.composer
            .queue_attachment("missing.png".into(), wr::FileKind::Image);
        app.composer
            .queue_attachment("notes.pdf".into(), wr::FileKind::Document);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                crate::ui::render_chats_with_plan(
                    frame,
                    &mut app,
                    &mut crate::app::events::MediaRenderPlan::default(),
                    &mut crate::app::read_receipts::VisibilityPlan::default(),
                    Rect::new(0, 0, width, height),
                );
            })
            .unwrap();
        let output = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(output.contains("caption"), "{width}x{height}: {output}");
        if height == 20 {
            assert!(output.contains("Image: missing.png"));
            assert!(output.contains("Document: notes.pdf"));
        }
        assert!(
            app.composer_previews.entries.is_empty(),
            "render must not schedule work"
        );
        let outcome = app.composer.apply(ComposerAction::Submit);
        assert_eq!(outcome.file_messages().len(), 2);
        assert_eq!(outcome.file_messages()[0].path.as_ref(), "missing.png");
        assert_eq!(
            outcome.file_messages()[0].caption.as_deref(),
            Some("caption")
        );
        assert_eq!(outcome.file_messages()[1].kind, wr::FileKind::Document);
        assert_eq!(outcome.file_messages()[1].caption, None);
    }
}

#[test]
fn bounded_image_decode_fails_closed_and_never_writes_sidecars() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("photo.png");
    image::DynamicImage::new_rgb8(8, 8).save(&path).unwrap();
    let original = std::fs::read(&path).unwrap();
    let attachment = PendingAttachment::new(
        path.to_string_lossy().into_owned().into(),
        wr::FileKind::Image,
    );
    assert!(load_preview(directory.path(), &attachment).is_some());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(!path.with_extension("jpg").exists());
    std::fs::write(&path, b"corrupt").unwrap();
    assert!(load_preview(directory.path(), &attachment).is_none());
    let missing = PendingAttachment::new("missing.png".into(), wr::FileKind::Image);
    assert!(load_preview(directory.path(), &missing).is_none());
    let traversal = PendingAttachment::new("../photo.png".into(), wr::FileKind::Image);
    assert!(load_preview(directory.path(), &traversal).is_none());
    image::DynamicImage::new_rgb8(8193, 1).save(&path).unwrap();
    assert!(load_preview(directory.path(), &attachment).is_none());
    let video = PendingAttachment::new("missing.mp4".into(), wr::FileKind::Video);
    assert!(load_preview(directory.path(), &video).is_none());
    assert!(!directory.path().join("missing.jpg").exists());
}

#[test]
fn thumbnail_rows_are_optional_and_nonvisual_drafts_keep_their_layout() {
    let image = PendingAttachment::new("photo.png".into(), wr::FileKind::Image);
    let video = PendingAttachment::new("clip.mp4".into(), wr::FileKind::Video);
    let document = PendingAttachment::new("notes.pdf".into(), wr::FileKind::Document);
    for attachment in [image, video] {
        assert_eq!(preview_rows(std::slice::from_ref(&attachment), 16, 12), 4);
        assert_eq!(preview_rows(std::slice::from_ref(&attachment), 15, 12), 0);
        assert_eq!(preview_rows(std::slice::from_ref(&attachment), 16, 11), 0);
    }
    assert_eq!(preview_rows(&[], 60, 20), 0);
    assert_eq!(preview_rows(&[document], 60, 20), 0);
    let mut app = TestApp::new();
    app.composer.insert_text("plain text");
    app.composer
        .queue_attachment("notes.pdf".into(), wr::FileKind::Document);
    assert!(app.reconcile_composer_previews().is_empty());
    assert_eq!(app.composer.text(), "plain text");
    let outcome = app.composer.apply(ComposerAction::Submit);
    assert_eq!(outcome.file_messages()[0].path.as_ref(), "notes.pdf");
    assert_eq!(
        outcome.file_messages()[0].caption.as_deref(),
        Some("plain text")
    );
}

#[test]
fn ready_image_and_static_video_protocols_paint_in_the_composer() {
    use ratatui::style::Color;
    use ratatui_image::picker::Picker;

    let mut app = TestApp::new();
    app.open_chat = Some("alice@s.whatsapp.net".to_owned().into());
    app.conversation_mode = ConversationMode::ComposerEditing;
    app.composer.insert_text("caption");
    app.composer
        .queue_attachment("photo.png".into(), wr::FileKind::Image);
    app.composer
        .queue_attachment("clip.mp4".into(), wr::FileKind::Video);
    let requests = app.reconcile_composer_previews();
    let picker = Picker::halfblocks();
    let results = requests
        .iter()
        .zip([[240, 20, 30], [20, 30, 240]])
        .map(|(request, rgb)| {
            let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                32,
                16,
                image::Rgb(rgb),
            ));
            let mut protocol = picker.new_resize_protocol(image);
            protocol.resize_encode(
                &Resize::Scale(None),
                Size::new(PREVIEW_WIDTH, PREVIEW_HEIGHT),
            );
            (request.generation, Some(protocol))
        })
        .collect();
    assert!(app.finish_composer_previews(results));
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
    terminal
        .draw(|frame| {
            crate::ui::render_chats_with_plan(
                frame,
                &mut app,
                &mut crate::app::events::MediaRenderPlan::default(),
                &mut crate::app::read_receipts::VisibilityPlan::default(),
                Rect::new(0, 0, 60, 20),
            );
        })
        .unwrap();
    for color in [Color::Rgb(240, 20, 30), Color::Rgb(20, 30, 240)] {
        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .any(|cell| cell.fg == color || cell.bg == color)
        );
    }
    assert_eq!(app.composer.text(), "caption");
    assert!(app.reconcile_composer_previews().is_empty());
}
