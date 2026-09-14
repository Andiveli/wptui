use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use wp_tui::app::read_receipts::VisibilityPlan;
use wp_tui::app::{
    actions::{ActionNotice, AppAction, UrlOpener},
    events::MediaRenderPlan,
};
use wp_tui::url::{extract_openable_urls, url_launch_plan};
mod common;
use common::TestApp;

#[derive(Clone, Default)]
struct RecordingUrlOpener {
    plans: Rc<RefCell<Vec<wp_tui::url::UrlLaunchPlan>>>,
    failure: Option<io::ErrorKind>,
}

impl UrlOpener for RecordingUrlOpener {
    fn open(&mut self, plan: &wp_tui::url::UrlLaunchPlan) -> io::Result<()> {
        self.plans.borrow_mut().push(plan.clone());
        match self.failure {
            Some(kind) => Err(io::Error::from(kind)),
            None => Ok(()),
        }
    }
}

fn message(id: &str, text: &str) -> whatsrust::Message {
    whatsrust::Message {
        info: whatsrust::MessageInfo {
            id: id.into(),
            chat: "chat@example.test".to_owned().into(),
            sender: "sender@example.test".to_owned().into(),
            mentions_self: false,
            timestamp: 0,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: whatsrust::MessageContent::Text(text.into()),
    }
}

fn selected_app(message: whatsrust::Message) -> TestApp {
    let mut app = TestApp::new();
    app.message_list_state
        .set_selected_message(message.info.id.clone());
    app.messages.insert(message.info.id.clone(), message);
    app
}

#[test]
fn extracts_http_urls_with_sentence_punctuation_and_rejects_non_web_schemes() {
    assert_eq!(
        extract_openable_urls(
            "See (https://example.test/a?x=1), then http://example.test/b. javascript:alert(1) javascript:https://evil.example file:///tmp/x",
        ),
        vec![
            "https://example.test/a?x=1".to_owned(),
            "http://example.test/b".to_owned(),
        ]
    );
}

#[test]
fn opening_one_text_url_uses_a_structured_launch_plan() {
    let opener = RecordingUrlOpener::default();
    let plans = opener.plans.clone();
    let mut app = selected_app(message("one", "Read https://example.test/docs."));
    app.url_opener = Box::new(opener);

    app.dispatch_action(AppAction::OpenMessage);

    assert_eq!(
        plans.borrow().as_slice(),
        &[url_launch_plan("https://example.test/docs")]
    );
    assert!(app.url_picker.is_none());
}

#[test]
fn multiple_urls_open_a_picker_and_confirming_the_selected_url_launches_it() {
    let opener = RecordingUrlOpener::default();
    let plans = opener.plans.clone();
    let mut app = selected_app(message(
        "many",
        "First https://one.example.test and second https://two.example.test/path",
    ));
    app.url_opener = Box::new(opener);

    app.dispatch_action(AppAction::OpenMessage);
    assert_eq!(
        app.url_picker
            .as_ref()
            .map(|(urls, selected)| (urls.as_slice(), *selected)),
        Some((
            [
                "https://one.example.test".to_owned(),
                "https://two.example.test/path".to_owned(),
            ]
            .as_slice(),
            0,
        ))
    );
    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
    app.on_terminal_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));

    assert_eq!(
        plans.borrow().as_slice(),
        &[url_launch_plan("https://two.example.test/path")]
    );
    assert!(app.url_picker.is_none());
}

#[test]
fn picker_cancel_preserves_the_selected_message_without_launching() {
    let opener = RecordingUrlOpener::default();
    let plans = opener.plans.clone();
    let mut app = selected_app(message(
        "many",
        "https://one.example.test https://two.example.test",
    ));
    app.url_opener = Box::new(opener);

    app.dispatch_action(AppAction::OpenMessage);
    app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));

    assert!(plans.borrow().is_empty());
    assert_eq!(
        app.message_list_state.get_selected_message().as_deref(),
        Some("many")
    );
    assert_eq!(app.action_notice, Some(ActionNotice::Cancelled));
}

#[test]
fn caption_url_opens_and_opener_failure_surfaces_a_notice() {
    let opener = RecordingUrlOpener {
        failure: Some(io::ErrorKind::NotFound),
        ..Default::default()
    };
    let mut file = message("caption", "");
    file.message = whatsrust::MessageContent::File(whatsrust::FileContent {
        caption: Some("https://caption.example.test".into()),
        ..Default::default()
    });
    let mut app = selected_app(file);
    app.url_opener = Box::new(opener);

    app.dispatch_action(AppAction::OpenMessage);

    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable("Could not open URL".into()))
    );
}

#[test]
fn document_caption_urls_take_precedence_over_external_document_opening() {
    let opener = RecordingUrlOpener::default();
    let plans = opener.plans.clone();
    let mut file = message("document", "");
    file.message = whatsrust::MessageContent::File(whatsrust::FileContent {
        kind: whatsrust::FileKind::Document,
        path: "report.pdf".into(),
        caption: Some("https://caption.example.test".into()),
        ..Default::default()
    });
    let mut app = selected_app(file);
    app.url_opener = Box::new(opener);
    app.metadata.insert(
        "document".into(),
        wp_tui::app::Metadata::File(wp_tui::app::FileMeta::Downloaded),
    );

    app.dispatch_action(AppAction::OpenMessage);

    assert_eq!(
        plans.borrow().as_slice(),
        &[url_launch_plan("https://caption.example.test")]
    );
    assert!(app.url_picker.is_none());
}

#[test]
fn unready_document_opening_uses_the_existing_media_unavailable_notice() {
    let mut file = message("document", "");
    file.message = whatsrust::MessageContent::File(whatsrust::FileContent {
        kind: whatsrust::FileKind::Document,
        path: "report.pdf".into(),
        ..Default::default()
    });
    let mut app = selected_app(file);

    app.dispatch_action(AppAction::OpenMessage);

    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable("Media is not downloaded".into()))
    );
}

#[test]
fn picker_renders_the_selected_url_and_navigation_hint() {
    let mut app = selected_app(message(
        "many",
        "https://one.example.test https://two.example.test",
    ));
    app.dispatch_action(AppAction::OpenMessage);
    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            let mut media_render_plan = MediaRenderPlan::default();
            let mut visibility_plan = VisibilityPlan::default();
            wp_tui::ui::draw_with_plan(
                frame,
                &mut app,
                &mut media_render_plan,
                &mut visibility_plan,
            )
        })
        .unwrap();

    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(rendered.contains("Open link"));
    assert!(rendered.contains("https://one.example.test"));
    assert!(rendered.contains("Enter open"));
}

#[test]
fn no_supported_url_surfaces_the_existing_unavailable_notice() {
    let mut app = selected_app(message("none", "javascript:alert(1)"));

    app.dispatch_action(AppAction::OpenMessage);

    assert_eq!(
        app.action_notice,
        Some(ActionNotice::Unavailable("Open is not available".into()))
    );
}
