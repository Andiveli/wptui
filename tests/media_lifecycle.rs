use std::{
    fs,
    path::{Path, PathBuf},
    process::Child,
    sync::{Arc, Mutex},
};

use tempfile::tempdir;
use whatsrust::{FileKind, JID, Message, MessageContent, MessageInfo};
use wp_tui::app::actions::{ActionNotice, AppAction};
use wp_tui::app::events::{
    ViewerPreviewKey, ViewerPreviewState, ViewerStatus, viewer_preview_request,
};
use wp_tui::media::{
    ChildReaper, CommandLaunchExecutor, LaunchExecutor, MediaLaunchError, MediaRoot, StandardIo,
    command_spec, document_opener_from_environment, execute_launch, media_opener_from_environment,
    media_player_from_environment, plan_media_launch, resolve_document_opener,
    resolve_media_opener, resolve_media_player,
};
use wp_tui::ui::{composer_cursor_position, viewer_preview_layout};
mod common;
use common::TestApp;

#[test]
fn viewer_preview_requests_are_dimension_aware_deduplicated_and_failure_stable() {
    let mut state = None;
    let initial = ViewerPreviewKey::new("image.png", 76, 13);

    assert_eq!(
        viewer_preview_request(&mut state, initial.clone()),
        Some(initial.clone())
    );
    assert_eq!(viewer_preview_request(&mut state, initial.clone()), None);

    state = Some(ViewerPreviewState::Failed(initial.clone()));
    assert_eq!(viewer_preview_request(&mut state, initial), None);
    assert_eq!(
        viewer_preview_request(&mut state, ViewerPreviewKey::new("image.png", 40, 10)),
        Some(ViewerPreviewKey::new("image.png", 40, 10))
    );
}

#[test]
fn video_viewer_preview_uses_the_chat_thumbnail_sidecar() {
    let key = ViewerPreviewKey::for_attachment("nested/clip.mp4", FileKind::Video, 76, 13);

    assert_eq!(key.preview_path().as_ref(), "nested/clip.jpg");
    assert!(key.is_video);

    let image = ViewerPreviewKey::for_attachment("image.png", FileKind::Image, 76, 13);
    assert_eq!(image.preview_path().as_ref(), "image.png");
    assert!(!image.is_video);
}

#[test]
fn viewer_layout_centers_and_clamps_preview_after_resize() {
    let large = viewer_preview_layout(ratatui::layout::Rect::new(0, 0, 100, 40), 100);
    assert_eq!(large.preview, ratatui::layout::Rect::new(10, 5, 80, 27));

    let compact = viewer_preview_layout(ratatui::layout::Rect::new(0, 0, 40, 20), 100);
    assert_eq!(compact.preview, ratatui::layout::Rect::new(5, 4, 29, 10));
}

#[test]
fn viewer_layout_clamps_zoom_below_the_minimum() {
    let minimum = viewer_preview_layout(ratatui::layout::Rect::new(0, 0, 100, 40), 25);
    let below_minimum = viewer_preview_layout(ratatui::layout::Rect::new(0, 0, 100, 40), 0);

    assert_eq!(below_minimum, minimum);
    assert_eq!(
        below_minimum.preview,
        ratatui::layout::Rect::new(40, 15, 20, 7)
    );
}

#[test]
fn viewer_layout_keeps_tiny_and_empty_areas_inside_the_parent() {
    for area in [
        ratatui::layout::Rect::new(0, 0, 0, 0),
        ratatui::layout::Rect::new(0, 0, 3, 3),
        ratatui::layout::Rect::new(5, 7, 4, 2),
    ] {
        let layout = viewer_preview_layout(area, 400);
        let inside = |rect: ratatui::layout::Rect| {
            rect.is_empty()
                || (rect.x >= area.x
                    && rect.y >= area.y
                    && rect.right() <= area.right()
                    && rect.bottom() <= area.bottom())
        };

        assert!(inside(layout.modal));
        assert!(inside(layout.body));
        assert!(inside(layout.hint));
        assert!(inside(layout.preview));
    }
}

#[test]
fn composer_cursor_position_is_relative_to_the_final_input_area() {
    assert_eq!(
        composer_cursor_position(ratatui::layout::Rect::new(10, 20, 30, 4), (2, 7)),
        ratatui::layout::Position::new(17, 22)
    );
}

#[test]
fn viewer_collects_chat_images_and_videos_at_the_selected_item_and_navigates_bounded() {
    let mut app = TestApp::new();
    let image = media_message("image", FileKind::Image, "image.png");
    let selected = media_message("selected", FileKind::Video, "clip.mp4");
    let audio = media_message("audio", FileKind::Audio, "voice.ogg");
    let chat = selected.info.chat.clone();
    for message in [image, selected, audio] {
        app.chat_messages
            .entry(chat.clone())
            .or_default()
            .push(message.info.id.clone());
        app.messages.insert(message.info.id.clone(), message);
    }
    app.message_list_state
        .set_selected_message("selected".into());
    app.dispatch_action(AppAction::ViewMessage);
    let viewer = app.attachment_viewer.as_ref().expect("viewer opens");
    assert_eq!((viewer.attachment_count, viewer.index), (2, 1));
    assert_eq!(viewer.message_id.as_ref(), "selected");
    app.dispatch_action(AppAction::ViewerNext);
    assert_eq!(app.attachment_viewer.as_ref().unwrap().index, 1);
    app.dispatch_action(AppAction::ViewerPrevious);
    assert_eq!(
        app.attachment_viewer.as_ref().unwrap().message_id.as_ref(),
        "image"
    );
}

#[test]
fn viewer_closes_and_reports_missing_or_unsupported_media_without_losing_usability() {
    let mut app = TestApp::new();
    let missing = media_message("missing", FileKind::Image, "missing.png");
    app.messages.insert(missing.info.id.clone(), missing);
    app.message_list_state
        .set_selected_message("missing".into());
    app.dispatch_action(AppAction::ViewMessage);
    assert_eq!(
        app.attachment_viewer.as_ref().unwrap().status,
        ViewerStatus::Missing
    );
    assert_eq!(
        app.attachment_viewer.as_ref().unwrap().message_id.as_ref(),
        "missing"
    );
    app.dispatch_action(AppAction::CloseAttachmentViewer);
    assert!(app.attachment_viewer.is_none());

    let sticker = media_message("sticker", FileKind::Sticker, "sticker.webp");
    app.messages.insert(sticker.info.id.clone(), sticker);
    app.message_list_state
        .set_selected_message("sticker".into());
    app.dispatch_action(AppAction::ViewMessage);
    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unsupported(
            "Sticker viewer is not supported".into()
        ))
    );
}

#[test]
fn viewer_launch_rejects_unready_media_with_a_visible_notice() {
    let mut app = TestApp::new();
    let media = media_message("missing", FileKind::Video, "missing.mp4");
    app.messages.insert(media.info.id.clone(), media);
    app.message_list_state
        .set_selected_message("missing".into());
    app.dispatch_action(AppAction::ViewMessage);

    app.on_terminal_event(ratatui::crossterm::event::Event::Key(
        ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('x'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ),
    ));

    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable("Media is not downloaded".into()))
    );
}

#[test]
fn media_failure_from_the_terminal_event_path_produces_a_visible_notice() {
    let root = tempdir().unwrap();
    let mut app = TestApp::new();
    app.focus_pane = wp_tui::app::actions::FocusPane::Conversation;
    app.media_path = root.path().to_path_buf();
    let media = media_message("missing", FileKind::Video, "missing.mp4");
    app.messages.insert(media.info.id.clone(), media);
    app.message_list_state
        .set_selected_message("missing".into());
    app.metadata.insert(
        "missing".into(),
        wp_tui::app::Metadata::File(wp_tui::app::FileMeta::Downloaded),
    );

    app.on_terminal_event(ratatui::crossterm::event::Event::Key(
        ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('x'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ),
    ));

    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable(
            "Media launch is unavailable".into()
        ))
    );
}

fn media_message(id: &str, kind: FileKind, path: &str) -> Message {
    let chat = JID::from("chat@example.test".to_owned());
    Message {
        info: MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: chat,
            timestamp: 0,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: MessageContent::File(whatsrust::FileContent {
            path: path.into(),
            kind,
            caption: None,
            ..Default::default()
        }),
    }
}

#[test]
fn traversal_and_absolute_paths_are_rejected_without_a_launch_plan() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("outside.mp4"), []).unwrap();
    let media = MediaRoot::new(root.path()).unwrap();

    assert_eq!(
        media.plan_launch(Path::new("mpv"), Path::new("../outside.mp4")),
        Err(MediaLaunchError::ParentTraversal)
    );
    assert_eq!(
        media.plan_launch(
            Path::new("mpv"),
            outside.path().join("outside.mp4").as_path()
        ),
        Err(MediaLaunchError::AbsolutePath)
    );
}

#[test]
fn malformed_paths_are_rejected_but_nested_media_paths_remain_usable() {
    let root = tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::write(root.path().join("nested/clip.mp4"), []).unwrap();
    let media = MediaRoot::new(root.path()).unwrap();

    assert_eq!(
        media.plan_launch(Path::new("mpv"), Path::new("")),
        Err(MediaLaunchError::MalformedPath)
    );
    assert!(
        media
            .plan_launch(Path::new("mpv"), Path::new("nested/clip.mp4"))
            .is_ok()
    );
}

#[test]
fn missing_files_and_unsafe_executables_produce_no_launch_plan() {
    let root = tempdir().unwrap();
    let media = MediaRoot::new(root.path()).unwrap();
    fs::create_dir(root.path().join("directory.mp4")).unwrap();

    assert_eq!(
        media.plan_launch(Path::new("mpv"), Path::new("missing.mp4")),
        Err(MediaLaunchError::MissingFile)
    );
    assert_eq!(
        media.plan_launch(Path::new("mpv"), Path::new("directory.mp4")),
        Err(MediaLaunchError::NotFile)
    );
    assert_eq!(
        media.plan_launch(Path::new("/bin/sh"), Path::new("missing.mp4")),
        Err(MediaLaunchError::UnsafeExecutable)
    );
}

#[cfg(unix)]
#[test]
fn symlink_escapes_are_rejected_without_a_launch_plan() {
    use std::os::unix::fs::symlink;

    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let escaped = outside.path().join("outside.mp4");
    fs::write(&escaped, []).unwrap();
    symlink(&escaped, root.path().join("escape.mp4")).unwrap();
    let media = MediaRoot::new(root.path()).unwrap();

    assert_eq!(
        media.plan_launch(Path::new("mpv"), Path::new("escape.mp4")),
        Err(MediaLaunchError::SymlinkEscape)
    );
}

#[test]
fn metacharacter_filenames_remain_one_literal_argument() {
    let root = tempdir().unwrap();
    let name = "movie;touch pwn.mp4";
    fs::write(root.path().join(name), []).unwrap();
    let media = MediaRoot::new(root.path()).unwrap();

    let plan = media
        .plan_launch(Path::new("mpv"), Path::new(name))
        .unwrap();

    assert_eq!(plan.executable(), Path::new("mpv"));
    assert_eq!(plan.arguments()[0], PathBuf::from("--force-window"));
    assert_eq!(plan.arguments()[1], PathBuf::from("--keep-open=yes"));
    assert!(plan.arguments()[2].starts_with("/proc/self/fd/"));
}

#[test]
fn unavailable_configuration_or_cancellation_returns_no_launch_plan() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("movie.mp4"), []).unwrap();

    assert_eq!(
        plan_media_launch(None, Some(Path::new("mpv")), Some(Path::new("movie.mp4"))),
        Err(MediaLaunchError::UnconfiguredMediaRoot)
    );
    assert_eq!(
        plan_media_launch(Some(root.path()), None, Some(Path::new("movie.mp4"))),
        Err(MediaLaunchError::PlayerUnavailable)
    );
    assert_eq!(
        plan_media_launch(Some(root.path()), Some(Path::new("mpv")), None),
        Ok(None)
    );
}

#[test]
fn media_launch_passes_one_canonical_argument_to_a_structured_executor() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("clip.mp4"), []).unwrap();
    let plan = MediaRoot::new(root.path())
        .unwrap()
        .plan_launch(Path::new("mpv"), Path::new("clip.mp4"))
        .unwrap();
    let mut executor = RecordingExecutor::default();

    execute_launch(&plan, &mut executor).unwrap();

    assert_eq!(executor.calls[0].0, PathBuf::from("mpv"));
    assert_eq!(executor.calls[0].1.len(), 3);
    assert_eq!(executor.calls[0].1[0], PathBuf::from("--force-window"));
    assert_eq!(executor.calls[0].1[1], PathBuf::from("--keep-open=yes"));
    assert!(executor.calls[0].1[2].starts_with("/proc/self/fd/"));
}

#[cfg(unix)]
#[test]
fn media_launch_keeps_a_stable_contained_file_when_the_original_path_is_replaced() {
    use std::os::unix::fs::symlink;

    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let target = root.path().join("clip.mp4");
    fs::write(&target, b"inside").unwrap();
    let plan = MediaRoot::new(root.path())
        .unwrap()
        .plan_launch(Path::new("mpv"), Path::new("clip.mp4"))
        .unwrap();
    fs::remove_file(&target).unwrap();
    symlink(outside.path().join("outside.mp4"), &target).unwrap();
    fs::write(outside.path().join("outside.mp4"), b"outside").unwrap();
    let mut executor = RecordingExecutor::default();

    execute_launch(&plan, &mut executor).unwrap();

    assert_eq!(fs::read(&executor.calls[0].1[2]).unwrap(), b"inside");
}

#[test]
fn document_openers_select_pdf_case_insensitively_and_keep_paths_as_safe_arguments() {
    assert_eq!(
        resolve_document_opener(Path::new("report.pdf"), None, None),
        Ok(PathBuf::from("zathura"))
    );
    assert_eq!(
        resolve_document_opener(Path::new("REPORT.PDF"), None, None),
        Ok(PathBuf::from("zathura"))
    );
    assert_eq!(
        resolve_document_opener(Path::new("notes.odt"), None, None),
        Ok(PathBuf::from("libreoffice"))
    );
    assert_eq!(
        resolve_document_opener(Path::new("report.pdf"), Some("/bin/sh".into()), None),
        Err(MediaLaunchError::UnsafeExecutable)
    );

    let root = tempdir().unwrap();
    let name = "report;touch pwn.PDF";
    fs::write(root.path().join(name), []).unwrap();
    let plan = MediaRoot::new(root.path())
        .unwrap()
        .plan_launch(Path::new("zathura"), Path::new(name))
        .unwrap();

    assert_eq!(plan.executable(), Path::new("zathura"));
    assert_eq!(plan.arguments().len(), 1);
    assert!(plan.arguments()[0].starts_with("/proc/self/fd/"));

    let name = "notes;touch pwn.odt";
    fs::write(root.path().join(name), []).unwrap();
    let plan = MediaRoot::new(root.path())
        .unwrap()
        .plan_launch(Path::new("libreoffice"), Path::new(name))
        .unwrap();

    assert_eq!(plan.executable(), Path::new("libreoffice"));
    assert_eq!(plan.arguments().len(), 1);
    assert!(plan.arguments()[0].starts_with("/proc/self/fd/"));
}

#[test]
fn image_opener_defaults_to_feh_while_other_media_uses_mpv() {
    assert_eq!(
        resolve_media_opener(&FileKind::Image, None, None),
        Ok(PathBuf::from("feh"))
    );
    assert_eq!(
        resolve_media_opener(
            &FileKind::Image,
            Some("image-viewer".into()),
            Some("mpv".into())
        ),
        Ok(PathBuf::from("image-viewer"))
    );
    assert_eq!(
        resolve_media_opener(&FileKind::Video, Some("feh".into()), None),
        Ok(PathBuf::from("mpv"))
    );
}

#[test]
fn environment_selects_media_player_and_openers_by_wptui_vars() {
    struct EnvGuard(&'static [&'static str]);
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for name in self.0 {
                unsafe { std::env::remove_var(name) };
            }
        }
    }

    let guard = EnvGuard(&[
        "WPTUI_MEDIA_PLAYER",
        "WPTUI_IMAGE_VIEWER",
        "WPTUI_PDF_VIEWER",
        "WPTUI_DOCUMENT_VIEWER",
    ]);
    unsafe {
        std::env::set_var("WPTUI_MEDIA_PLAYER", "wptui-media-test");
        std::env::set_var("WPTUI_IMAGE_VIEWER", "wptui-image-test");
        std::env::set_var("WPTUI_PDF_VIEWER", "wptui-pdf-test");
        std::env::set_var("WPTUI_DOCUMENT_VIEWER", "wptui-doc-test");
    }

    assert_eq!(
        media_player_from_environment(),
        Ok(PathBuf::from("wptui-media-test"))
    );
    assert_eq!(
        media_opener_from_environment(&FileKind::Image),
        Ok(PathBuf::from("wptui-image-test"))
    );
    assert_eq!(
        media_opener_from_environment(&FileKind::Video),
        Ok(PathBuf::from("wptui-media-test"))
    );
    assert_eq!(
        document_opener_from_environment(Path::new("report.pdf")),
        Ok(PathBuf::from("wptui-pdf-test"))
    );
    assert_eq!(
        document_opener_from_environment(Path::new("notes.odt")),
        Ok(PathBuf::from("wptui-doc-test"))
    );

    drop(guard);
}

#[test]
fn feh_opens_one_stable_image_after_an_option_terminator() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("--image.png"), []).unwrap();

    let plan = MediaRoot::new(root.path())
        .unwrap()
        .plan_launch(Path::new("feh"), Path::new("--image.png"))
        .unwrap();

    assert_eq!(plan.arguments()[0], PathBuf::from("--"));
    assert_eq!(plan.arguments().len(), 2);
    assert!(plan.arguments()[1].starts_with("/proc/self/fd/"));
}

#[test]
fn exact_mpv_gets_keep_open_override_but_custom_players_do_not() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("clip.mp4"), []).unwrap();
    let media = MediaRoot::new(root.path()).unwrap();

    let mpv = media
        .plan_launch(Path::new("/opt/players/mpv"), Path::new("clip.mp4"))
        .unwrap();
    assert_eq!(mpv.arguments()[0], PathBuf::from("--force-window"));
    assert_eq!(mpv.arguments()[1], PathBuf::from("--keep-open=yes"));
    assert!(mpv.arguments()[2].starts_with("/proc/self/fd/"));
    let custom = media
        .plan_launch(Path::new("mpv-wrapper"), Path::new("clip.mp4"))
        .unwrap();
    assert_eq!(custom.arguments().len(), 1);
    assert!(custom.arguments()[0].starts_with("/proc/self/fd/"));
    assert_eq!(resolve_media_player(None), Ok(PathBuf::from("mpv")));
}

#[test]
fn command_spec_detaches_standard_streams_and_uses_a_process_group() {
    let spec = command_spec(Path::new("true"), &[]);

    assert_eq!(spec.executable(), Path::new("true"));
    assert_eq!(spec.arguments(), &[] as &[PathBuf]);
    assert_eq!(spec.stdin, StandardIo::Null);
    assert_eq!(spec.stdout, StandardIo::Null);
    assert_eq!(spec.stderr, StandardIo::Null);
    #[cfg(unix)]
    assert!(spec.separate_process_group);
}

#[cfg(unix)]
#[test]
fn short_lived_success_and_failure_children_are_handed_to_the_reaper() {
    for executable in ["true", "false"] {
        let handoff = Arc::new(Mutex::new(None));
        let mut reaper = RecordingChildReaper(handoff.clone());

        CommandLaunchExecutor::spawn_with_reaper(Path::new(executable), &[], &mut reaper).unwrap();

        let mut child = handoff.lock().unwrap().take().expect("child handoff");
        assert_eq!(child.wait().unwrap().success(), executable == "true");
    }
}

#[cfg(unix)]
#[test]
fn unavailable_player_reports_spawn_failure_without_a_shell() {
    let mut executor = CommandLaunchExecutor;

    let error = executor
        .spawn(Path::new("wptui-player-that-does-not-exist"), &[])
        .expect_err("missing executable must be reported");

    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[derive(Default)]
struct RecordingExecutor {
    calls: Vec<(PathBuf, Vec<PathBuf>)>,
}

impl LaunchExecutor for RecordingExecutor {
    fn spawn(&mut self, executable: &Path, arguments: &[PathBuf]) -> std::io::Result<()> {
        self.calls.push((executable.into(), arguments.to_vec()));
        Ok(())
    }
}

struct RecordingChildReaper(Arc<Mutex<Option<Child>>>);

impl ChildReaper for RecordingChildReaper {
    fn reap(&mut self, child: Child) {
        *self.0.lock().unwrap() = Some(child);
    }
}
