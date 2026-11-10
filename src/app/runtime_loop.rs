use std::sync::mpsc::{self, Sender};

use log::error;
use whatsrust as wr;

use crate::app::App;
use crate::app::download_worker::Worker as DownloadWorker;
use crate::app::events::{
    AppEvent, AppEventFamily, AppInput, DrawSource, MediaRenderEffect, MediaRenderPlan,
    viewer_preview_request,
};
use crate::app::media_jobs::MediaJobOwner;
use crate::app::terminal_session::TerminalSession;
use crate::ui;

type DownloadSender = Sender<(wr::MessageId, wr::FileId)>;

fn dispatch_app_event(
    app: &mut App<'_>,
    event: AppEvent,
    download_tx: &DownloadSender,
    media_jobs: &mut MediaJobOwner,
) -> bool {
    match event.family() {
        AppEventFamily::Send => app.handle_send_event(event),
        AppEventFamily::ReadReceipt => app.handle_read_receipt_event(event),
        AppEventFamily::Avatar => app.handle_avatar_event(event),
        AppEventFamily::Updater => app.handle_updater_event(event),
        AppEventFamily::MediaViewer => {
            app.handle_media_viewer_event(event, download_tx, media_jobs)
        }
    }
}

fn same_media_render_effect(left: &MediaRenderEffect, right: &MediaRenderEffect) -> bool {
    match (left, right) {
        (
            MediaRenderEffect::DownloadFile(left_id, _),
            MediaRenderEffect::DownloadFile(right_id, _),
        )
        | (
            MediaRenderEffect::LoadFilePreview(left_id),
            MediaRenderEffect::LoadFilePreview(right_id),
        ) => left_id == right_id,
        (
            MediaRenderEffect::LoadViewerPreview(left_key),
            MediaRenderEffect::LoadViewerPreview(right_key),
        ) => left_key == right_key,
        (
            MediaRenderEffect::LoadNativeLinkThumbnail(left),
            MediaRenderEffect::LoadNativeLinkThumbnail(right),
        ) => left == right,
        _ => false,
    }
}

fn dispatch_media_render_plan_after_draw<T, E>(
    app: &mut App<'_>,
    draw_result: Result<T, E>,
    plan: MediaRenderPlan,
    download_tx: &DownloadSender,
    media_jobs: &mut MediaJobOwner,
) {
    if draw_result.is_ok() {
        dispatch_media_render_plan(app, plan, download_tx, media_jobs);
    }
}

fn dispatch_media_render_plan(
    app: &mut App<'_>,
    plan: MediaRenderPlan,
    download_tx: &DownloadSender,
    media_jobs: &mut MediaJobOwner,
) {
    let effects = plan.into_effects();
    // Also reconcile an empty plan: cached/hidden panes must cancel old demand.
    let visible = effects
        .iter()
        .filter_map(|effect| match effect {
            MediaRenderEffect::LoadNativeLinkThumbnail(key)
                if app.native_thumbnail_is_current(key) =>
            {
                Some(key.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    app.native_link_thumbnails.reconcile(&visible);
    let mut dispatched = Vec::new();
    for effect in effects {
        if dispatched
            .iter()
            .any(|previous| same_media_render_effect(previous, &effect))
        {
            continue;
        }
        let event = match &effect {
            MediaRenderEffect::DownloadFile(message_id, file_id) => {
                AppEvent::DownloadFile(message_id.clone(), file_id.clone())
            }
            MediaRenderEffect::LoadFilePreview(message_id) => {
                AppEvent::LoadFilePreview(message_id.clone())
            }
            MediaRenderEffect::LoadNativeLinkThumbnail(key) => AppEvent::LoadNativeLinkThumbnail(
                crate::app::events::NativeLinkThumbnailRequest(key.clone()),
            ),
            MediaRenderEffect::LoadViewerPreview(key) => {
                let Some(key) = viewer_preview_request(&mut app.viewer_preview, key.clone()) else {
                    continue;
                };
                AppEvent::LoadViewerPreview(key)
            }
        };
        dispatched.push(effect);
        dispatch_app_event(app, event, download_tx, media_jobs);
    }
}

fn should_draw_for_source(_app: &App<'_>, source: DrawSource) -> bool {
    match source {
        DrawSource::Ordinary => true,
        DrawSource::GoLog => false,
    }
}

fn refresh_composer_viewport_width(app: &mut App<'_>, terminal_session: &mut TerminalSession) {
    let Ok(area) = terminal_session.terminal_mut().size() else {
        return;
    };
    let width = crate::ui::composer_viewport_width(area.width, area.height, app.pane_visibility);
    app.set_composer_viewport_width(width);
}

#[derive(Debug, PartialEq)]
enum TerminalInitializationFailureTeardown {
    StopDownloadWorker,
    StopReadReceiptWorker,
    StopReadSyncWorker,
    Disconnect,
    FinalizeDiagnostics,
}

fn finish_terminal_initialization_failure(
    mut teardown: impl FnMut(TerminalInitializationFailureTeardown),
) {
    teardown(TerminalInitializationFailureTeardown::StopDownloadWorker);
    teardown(TerminalInitializationFailureTeardown::StopReadReceiptWorker);
    teardown(TerminalInitializationFailureTeardown::StopReadSyncWorker);
    teardown(TerminalInitializationFailureTeardown::Disconnect);
    teardown(TerminalInitializationFailureTeardown::FinalizeDiagnostics);
}

fn execute_terminal_initialization_failure(
    app: &mut App<'_>,
    download_worker: &mut DownloadWorker,
) {
    finish_terminal_initialization_failure(|step| match step {
        TerminalInitializationFailureTeardown::StopDownloadWorker => {
            download_worker.shutdown();
        }
        TerminalInitializationFailureTeardown::StopReadReceiptWorker => {
            app.shutdown_read_receipt_worker();
        }
        TerminalInitializationFailureTeardown::StopReadSyncWorker => {
            app.shutdown_read_sync_worker();
        }
        TerminalInitializationFailureTeardown::Disconnect => app.lifecycle_control.disconnect(),
        TerminalInitializationFailureTeardown::FinalizeDiagnostics => {
            app.finalize_runtime_diagnostics();
        }
    });
}

/// Owns the terminal runtime: input pumping, event dispatch, redraws, and shutdown.
///
/// Bootstrap stays in `App::run`; this phase owns the already-created download
/// worker and delegates each event family to its focused runtime owner.
pub(crate) fn run(app: &mut App<'_>, mut download_worker: DownloadWorker) {
    let download_tx = download_worker.sender();
    let mut terminal_session = match TerminalSession::try_new(app.mouse_capture_enabled) {
        Ok(session) => session,
        Err(e) => {
            error!("Failed to initialize terminal UI: {e}");
            eprintln!("Failed to initialize terminal UI: {e}");
            let _ = app
                .message_action_diagnostics
                .write_report(std::io::stderr());
            app.news.star.shutdown();
            app.join_news_workers();
            execute_terminal_initialization_failure(app, &mut download_worker);
            return;
        }
    };

    let mut media_jobs = MediaJobOwner::new();
    terminal_session.start_input_reader(&mut app.input_reader, app.tx.clone());

    app.sync_selected_presence();
    app.schedule_composer_previews(&mut media_jobs);
    refresh_composer_viewport_width(app, &mut terminal_session);
    let initial_draw_started = app.runtime_diagnostics.draw_started();
    let mut media_render_plan = MediaRenderPlan::default();
    let mut visibility_plan = crate::app::read_receipts::VisibilityPlan::default();
    let draw_result = terminal_session
        .sync_paste_mode(app.accepts_terminal_paste())
        .and_then(|()| {
            terminal_session.terminal_mut().draw(|frame| {
                ui::draw_with_plan(frame, app, &mut media_render_plan, &mut visibility_plan)
            })
        });
    if let Err(error) = &draw_result {
        error!("Failed to draw terminal UI: {error}");
        app.news.star.shutdown();
        app.join_news_workers();
        app.shutdown_avatar_runtime();
        media_jobs.shutdown();
        download_worker.shutdown();
        app.shutdown_read_receipt_worker();
        app.shutdown_read_sync_worker();
        terminal_session.stop_input_reader(&mut app.input_reader);
        terminal_session.restore();
        app.lifecycle_control.disconnect();
        let _ = app
            .message_action_diagnostics
            .write_report(std::io::stderr());
        app.finalize_runtime_diagnostics();
        return;
    }
    app.runtime_diagnostics.record_should_draw();
    if let Some(started) = initial_draw_started {
        app.runtime_diagnostics.record_draw_finished(started);
    }
    app.apply_sticker_render_plan(&media_render_plan);
    dispatch_media_render_plan_after_draw(
        app,
        draw_result,
        media_render_plan,
        &download_tx,
        &mut media_jobs,
    );
    app.apply_visibility_plan(visibility_plan);
    if let Ok(area) = terminal_session.terminal_mut().size() {
        app.schedule_avatar_viewport(area.into());
    }
    if app.advance_visible_stickers(std::time::Instant::now())
        && app.tx.send(AppInput::Draw(DrawSource::Ordinary)).is_err()
    {
        error!("Failed to schedule sticker preview redraw");
    }
    app.dispatch_read_receipts();

    loop {
        let now = app.now();
        let sticker_timeout = app
            .sticker_deadline()
            .map(|deadline| deadline.saturating_duration_since(std::time::Instant::now()));
        let timeout = match (app.selected_presence.redraw_after(now), sticker_timeout) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        let msg = match timeout {
            Some(timeout) => match app.rx.recv_timeout(timeout) {
                Ok(input) => Ok(input),
                Err(mpsc::RecvTimeoutError::Timeout) => Ok(AppInput::Draw(DrawSource::Ordinary)),
                Err(mpsc::RecvTimeoutError::Disconnected) => Err(mpsc::RecvError),
            },
            None => app.rx.recv(),
        };
        // info!("Received message: {:?}", &msg);
        refresh_composer_viewport_width(app, &mut terminal_session);
        if let Ok(ref input) = msg {
            app.runtime_diagnostics.record_input(input);
        }
        let should_draw = match msg {
            Ok(AppInput::App(event)) => {
                dispatch_app_event(app, event, &download_tx, &mut media_jobs)
            }
            Ok(AppInput::WhatsApp(event)) => {
                if matches!(
                    &event,
                    wr::Event::LogoutResult(
                        wr::LogoutStatus::LoggedOut | wr::LogoutStatus::NotLoggedIn
                    )
                ) {
                    // This is terminal shutdown, not ordinary event handling: no job
                    // may access or publish into the media directory while logout clears it.
                    media_jobs.shutdown();
                    download_worker.shutdown();
                    app.shutdown_read_sync_worker();
                }
                app.handle_whatsapp_event(event)
            }
            Ok(AppInput::Message { message, is_sync }) => app.process_message(message, is_sync),
            Ok(AppInput::Presence(update)) => app.handle_presence_update(update),
            Ok(AppInput::Terminal(event)) => {
                app.on_terminal_event(event);
                true
            }
            Ok(AppInput::Draw(source)) => {
                app.runtime_diagnostics.record_draw_source(source);
                let should_draw = should_draw_for_source(app, source);
                if !should_draw && matches!(source, DrawSource::GoLog) {
                    app.runtime_diagnostics.record_go_log_draw_suppressed();
                }
                should_draw
            }
            Err(_) => {
                error!("Failed to receive input from channel");
                true
            }
        };

        if let Err(error) = terminal_session.sync_paste_mode(app.accepts_terminal_paste()) {
            error!("Failed to synchronize terminal paste mode: {error}");
            break;
        }

        app.activate_news_star();
        app.sync_selected_presence();
        if !app.should_quit && !app.logout_in_progress {
            app.schedule_composer_previews(&mut media_jobs);
        }

        let sticker_changed = app.advance_visible_stickers(std::time::Instant::now());
        if should_draw || sticker_changed {
            app.runtime_diagnostics.record_should_draw();
            let started = app.runtime_diagnostics.draw_started();
            let mut media_render_plan = MediaRenderPlan::default();
            let mut visibility_plan = crate::app::read_receipts::VisibilityPlan::default();
            let draw_result = terminal_session.terminal_mut().draw(|frame| {
                ui::draw_with_plan(frame, app, &mut media_render_plan, &mut visibility_plan)
            });
            if let Err(error) = &draw_result {
                error!("Failed to draw terminal UI: {error}");
                app.set_read_receipt_readiness(crate::app::read_receipts::Readiness::Disconnected);
                break;
            }
            if let Some(started) = started {
                app.runtime_diagnostics.record_draw_finished(started);
            }
            app.apply_sticker_render_plan(&media_render_plan);
            dispatch_media_render_plan_after_draw(
                app,
                draw_result,
                media_render_plan,
                &download_tx,
                &mut media_jobs,
            );
            app.apply_visibility_plan(visibility_plan);
            if app.advance_visible_stickers(std::time::Instant::now())
                && app.tx.send(AppInput::Draw(DrawSource::Ordinary)).is_err()
            {
                error!("Failed to schedule sticker preview redraw");
            }
            if let Ok(area) = terminal_session.terminal_mut().size() {
                app.schedule_avatar_viewport(area.into());
            }
        }
        app.dispatch_read_receipts();

        if app.should_quit {
            break;
        }
    }

    app.news.star.shutdown();
    app.join_news_workers();
    app.shutdown_avatar_runtime();
    media_jobs.shutdown();
    download_worker.shutdown();
    app.shutdown_read_receipt_worker();
    app.shutdown_read_sync_worker();
    terminal_session.stop_input_reader(&mut app.input_reader);
    terminal_session.restore();
    app.set_read_receipt_readiness(crate::app::read_receipts::Readiness::Disconnected);
    app.lifecycle_control.disconnect();
    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    app.write_presence_diagnostics(&mut stderr);
    drop(stderr);
    let _ = app
        .message_action_diagnostics
        .write_report(std::io::stderr());
    app.finalize_runtime_diagnostics();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use crate::app::events::AppEvent;
    use crate::app::test_support::{RecordingChatReadSyncPort, RecordingLifecycleControl, TestApp};

    struct NativeAdmissionFixture {
        fixture: crate::app::test_support::FixedPickerApp,
    }

    impl NativeAdmissionFixture {
        fn new() -> Self {
            use crate::app::test_support::{FixedPickerApp, message};
            use image::{DynamicImage, Rgb, RgbImage};

            let mut fixture = FixedPickerApp::new();
            let app = &mut fixture.app;

            let chat: wr::JID = "admission@example.test".to_owned().into();
            let source = "https://example.test/article?q=%2F#part";
            let mut item = message(&chat, "native-admission", 1_704_110_400);
            item.message = wr::MessageContent::Text(source.into());
            let mut bytes = std::io::Cursor::new(Vec::new());
            DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 8, Rgb([220, 30, 40])))
                .write_to(&mut bytes, image::ImageFormat::Jpeg)
                .unwrap();
            wr::store_native_link_preview(
                &item.info,
                source,
                wr::NativeLinkPreview {
                    matched_text: source.into(),
                    title: "Native admission".into(),
                    description: "Stored bytes only".into(),
                    jpeg_thumbnail: bytes.into_inner(),
                },
            );
            app.process_message(item, true);
            app.contacts
                .insert(chat.clone(), "Admission fixture".into());
            app.open_chat = Some(chat);
            app.focus_pane = crate::app::actions::FocusPane::Conversation;
            app.conversation_mode = crate::app::actions::ConversationMode::ComposerEditing;
            // Wrong-chat rejection uses a real, registered alternate conversation.
            let other: wr::JID = "other@example.test".to_owned().into();
            let mut item = message(&other, "other-admission", 1_704_110_401);
            item.message = wr::MessageContent::Text("Other conversation".into());
            app.add_message(item);
            Self { fixture }
        }
    }

    fn native_admission_draw(app: &mut App<'_>) -> (std::io::Result<()>, MediaRenderPlan) {
        use ratatui::{Terminal, backend::TestBackend};

        let mut terminal = Terminal::new(TestBackend::new(180, 50)).unwrap();
        let mut plan = MediaRenderPlan::default();
        let result = terminal
            .draw(|frame| {
                ui::draw_with_plan(
                    frame,
                    app,
                    &mut plan,
                    &mut crate::app::read_receipts::VisibilityPlan::default(),
                );
            })
            .map(|_| ())
            .map_err(|never| -> std::io::Error { match never {} });
        (result, plan)
    }

    #[derive(Debug)]
    struct NativeAdmissionObservation {
        case: &'static str,
        draws_succeeded: bool,
        visible_key_count: usize,
        initially_cold: bool,
        hidden_fresh: Option<(usize, usize, usize)>,
        state: Option<crate::app::native_link_thumbnails::EntryState>,
        outstanding: usize,
        cache_len: usize,
        file_metadata_len: usize,
        file_image_cache_len: usize,
        downloads: usize,
    }

    #[test]
    fn native_ui_plans_admit_visible_chats_and_communities_but_reject_hidden_or_stale_routes() {
        use crate::app::actions::Section;
        use crate::app::native_link_thumbnails::EntryState;

        let mut observations = Vec::new();
        for case in ["chats", "communities", "news", "logout", "wrong-chat"] {
            let mut fixture = NativeAdmissionFixture::new();
            let app = &mut fixture.fixture.app;
            let mut media_jobs = MediaJobOwner::new();
            let (download_tx, download_rx) = mpsc::channel();
            app.selected_section = if case == "communities" {
                Section::Communities
            } else {
                Section::Chats
            };

            // Inspect one actual UI plan, then draw again into a cold cache.
            // Never manufacture/relabel a key or rebuild the dispatched plan.
            let (inspection_result, inspection_plan) = native_admission_draw(app);
            let visible_keys = inspection_plan
                .into_effects()
                .into_iter()
                .filter_map(|effect| {
                    if let MediaRenderEffect::LoadNativeLinkThumbnail(key) = effect {
                        Some(key)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let mut draws_succeeded = inspection_result.is_ok();
            let initially_cold = app.native_link_thumbnails.len() == 0
                && app.native_link_thumbnails.outstanding() == 0;
            let mut hidden_fresh = None;
            if matches!(case, "news" | "logout") {
                app.selected_section = if case == "news" {
                    Section::News
                } else {
                    Section::Chats
                };
                app.rail_on_logout = case == "logout";
                let (result, plan) = native_admission_draw(app);
                draws_succeeded &= result.is_ok();
                let count = plan
                    .into_effects()
                    .into_iter()
                    .filter(|effect| {
                        matches!(effect, MediaRenderEffect::LoadNativeLinkThumbnail(_))
                    })
                    .count();
                let (result, untouched_plan) = native_admission_draw(app);
                draws_succeeded &= result.is_ok();
                dispatch_media_render_plan_after_draw(
                    app,
                    result,
                    untouched_plan,
                    &download_tx,
                    &mut media_jobs,
                );
                hidden_fresh = Some((
                    count,
                    app.native_link_thumbnails.outstanding(),
                    app.native_link_thumbnails.len(),
                ));
                // A real prior-visible plan must also be rejected after hiding.
                app.selected_section = Section::Chats;
                app.rail_on_logout = false;
            }
            let (draw_result, untouched_plan) = native_admission_draw(app);
            draws_succeeded &= draw_result.is_ok();
            match case {
                "news" => app.selected_section = Section::News,
                "logout" => app.rail_on_logout = true,
                "wrong-chat" => app.open_chat = Some("other@example.test".to_owned().into()),
                _ => {}
            }
            dispatch_media_render_plan_after_draw(
                app,
                draw_result,
                untouched_plan,
                &download_tx,
                &mut media_jobs,
            );
            // Publication cannot mutate the cache: no completion is consumed.
            // Capture before shutdown, then join BEFORE any rule assertion so
            // the expected Communities RED cannot leak a managed worker.
            let observed = NativeAdmissionObservation {
                case,
                draws_succeeded,
                visible_key_count: visible_keys.len(),
                initially_cold,
                hidden_fresh,
                state: visible_keys
                    .first()
                    .map(|key| app.native_link_thumbnails.state(key)),
                outstanding: app.native_link_thumbnails.outstanding(),
                cache_len: app.native_link_thumbnails.len(),
                file_metadata_len: app.metadata.len(),
                file_image_cache_len: app.image_cache.len(),
                downloads: download_rx.try_iter().count(),
            };
            media_jobs.shutdown();
            observations.push(observed);
        }
        // Every fresh fixture and managed owner has been torn down before RED.
        for observed in observations {
            let admitted = matches!(observed.case, "chats" | "communities");
            assert!(observed.draws_succeeded, "{observed:?}");
            assert!(observed.initially_cold, "{observed:?}");
            assert_eq!(observed.visible_key_count, 1, "{observed:?}");
            if matches!(observed.case, "news" | "logout") {
                assert_eq!(observed.hidden_fresh, Some((0, 0, 0)), "{observed:?}");
            }
            assert_eq!(
                observed.state,
                Some(if admitted {
                    EntryState::Loading
                } else {
                    EntryState::Absent
                }),
                "{observed:?}"
            );
            assert_eq!(observed.outstanding, usize::from(admitted), "{observed:?}");
            assert_eq!(observed.cache_len, usize::from(admitted), "{observed:?}");
            assert_eq!(observed.file_metadata_len, 0, "{observed:?}");
            assert_eq!(observed.file_image_cache_len, 0, "{observed:?}");
            assert_eq!(observed.downloads, 0, "{observed:?}");
        }
    }

    #[test]
    fn terminal_initialization_failure_preserves_the_existing_teardown_order() {
        let mut events = Vec::new();

        finish_terminal_initialization_failure(|step| events.push(step));

        assert_eq!(
            events,
            [
                TerminalInitializationFailureTeardown::StopDownloadWorker,
                TerminalInitializationFailureTeardown::StopReadReceiptWorker,
                TerminalInitializationFailureTeardown::StopReadSyncWorker,
                TerminalInitializationFailureTeardown::Disconnect,
                TerminalInitializationFailureTeardown::FinalizeDiagnostics,
            ]
        );
    }

    #[test]
    fn terminal_initialization_failure_stops_worker_disconnects_once_and_finalizes() {
        let mut app = TestApp::new();
        let trace = Arc::new(Mutex::new(Vec::new()));
        let read_sync = RecordingChatReadSyncPort::with_shutdown_trace(Arc::clone(&trace));
        let lifecycle = Arc::new(RecordingLifecycleControl::with_trace(Arc::clone(&trace)));
        app.set_chat_read_sync(Box::new(read_sync.clone()));
        app.set_lifecycle_control(Arc::clone(&lifecycle));
        let mut download_worker = app.take_media_download_worker();

        execute_terminal_initialization_failure(&mut app, &mut download_worker);

        assert!(
            download_worker
                .sender()
                .send(("terminal-init".into(), "file-id".into()))
                .is_err()
        );
        assert_eq!(*read_sync.shutdowns.lock().unwrap(), 1);
        assert_eq!(*lifecycle.disconnects.lock().unwrap(), 1);
        assert_eq!(
            *trace.lock().unwrap(),
            ["read-sync:stop", "lifecycle:disconnect"]
        );
    }

    #[test]
    fn send_events_route_to_the_send_handler() {
        let mut app = TestApp::new();
        let (download_tx, _download_rx) = mpsc::channel();
        let mut media_jobs = MediaJobOwner::new();

        assert!(!dispatch_app_event(
            &mut app,
            AppEvent::OutboundSendFailed { local_send_id: 1 },
            &download_tx,
            &mut media_jobs,
        ));
    }

    #[test]
    fn read_receipt_events_route_to_the_read_receipt_handler() {
        let mut app = TestApp::new();
        app.read_receipts.set_enabled(true);
        let (download_tx, _download_rx) = mpsc::channel();
        let mut media_jobs = MediaJobOwner::new();

        assert!(!dispatch_app_event(
            &mut app,
            AppEvent::ReadReceiptRestored(Ok(Vec::new())),
            &download_tx,
            &mut media_jobs,
        ));
    }

    #[test]
    fn updater_events_route_to_the_updater_handler() {
        let mut app = TestApp::new();
        let (download_tx, _download_rx) = mpsc::channel();
        let mut media_jobs = MediaJobOwner::new();

        assert!(dispatch_app_event(
            &mut app,
            AppEvent::UpdateAvailable("1.2.3".to_owned()),
            &download_tx,
            &mut media_jobs,
        ));
        assert_eq!(app.update_notice.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn stale_media_viewer_requests_route_without_spawning_work() {
        let mut app = TestApp::new();
        let current = crate::app::events::ViewerPreviewKey::new("current.jpg", 20, 10);
        app.viewer_preview = Some(crate::app::events::ViewerPreviewState::Loading(
            current.clone(),
        ));
        let (download_tx, _download_rx) = mpsc::channel();
        let mut media_jobs = MediaJobOwner::new();

        assert!(!dispatch_app_event(
            &mut app,
            AppEvent::LoadViewerPreview(crate::app::events::ViewerPreviewKey::new(
                "stale.jpg",
                20,
                10,
            )),
            &download_tx,
            &mut media_jobs,
        ));
        assert_eq!(app.viewer_preview.as_ref().unwrap().key(), &current);
    }

    #[test]
    fn stale_media_viewer_results_route_without_mutating_preview_state() {
        let mut app = TestApp::new();
        let current = crate::app::events::ViewerPreviewKey::new("current.jpg", 20, 10);
        app.viewer_preview = Some(crate::app::events::ViewerPreviewState::Loading(
            current.clone(),
        ));
        let (download_tx, _download_rx) = mpsc::channel();
        let mut media_jobs = MediaJobOwner::new();

        assert!(!dispatch_app_event(
            &mut app,
            AppEvent::SetViewerPreview(
                crate::app::events::ViewerPreviewKey::new("stale.jpg", 20, 10),
                None,
            ),
            &download_tx,
            &mut media_jobs,
        ));
        assert_eq!(app.viewer_preview.as_ref().unwrap().key(), &current);
    }

    #[test]
    fn go_log_draws_are_suppressed_without_a_panel() {
        let app = TestApp::new();

        assert!(!should_draw_for_source(&app, DrawSource::GoLog));
        assert!(should_draw_for_source(&app, DrawSource::Ordinary));
    }

    #[test]
    fn draw_event_order_is_preserved_while_go_logs_are_suppressed() {
        let app = TestApp::new();

        let results = [DrawSource::GoLog, DrawSource::Ordinary, DrawSource::GoLog]
            .map(|source| should_draw_for_source(&app, source));

        assert_eq!(results, [false, true, false]);
    }

    #[test]
    fn dispatching_duplicate_download_effects_submits_one_download() {
        let mut app = TestApp::new();
        let (download_tx, download_rx) = mpsc::channel();
        let mut media_jobs = MediaJobOwner::new();
        let message_id: wr::MessageId = "duplicate-media".into();
        let file_id: wr::FileId = "file-id".into();
        let mut plan = MediaRenderPlan::default();
        plan.append(MediaRenderEffect::DownloadFile(
            message_id.clone(),
            file_id.clone(),
        ));
        plan.append(MediaRenderEffect::DownloadFile(message_id, file_id));

        dispatch_media_render_plan(&mut app, plan, &download_tx, &mut media_jobs);

        assert_eq!(download_rx.try_iter().count(), 1);
    }

    #[test]
    fn failed_media_render_attempt_leaves_viewer_unchanged_until_a_successful_plan_dispatches() {
        let mut app = TestApp::new();
        let (download_tx, _) = mpsc::channel();
        let mut media_jobs = MediaJobOwner::new();
        let key = crate::app::events::ViewerPreviewKey::new("media.png", 20, 8);
        let mut failed_plan = MediaRenderPlan::default();
        failed_plan.append(MediaRenderEffect::LoadViewerPreview(key.clone()));

        dispatch_media_render_plan_after_draw(
            &mut app,
            Err::<(), _>(()),
            failed_plan,
            &download_tx,
            &mut media_jobs,
        );
        assert!(app.viewer_preview.is_none());

        let mut successful_plan = MediaRenderPlan::default();
        successful_plan.append(MediaRenderEffect::LoadViewerPreview(key.clone()));
        dispatch_media_render_plan_after_draw(
            &mut app,
            Ok::<(), ()>(()),
            successful_plan,
            &download_tx,
            &mut media_jobs,
        );

        assert!(matches!(
            app.viewer_preview,
            Some(crate::app::events::ViewerPreviewState::Loading(ref actual_key))
                if actual_key == &key
        ));
    }
}
