use std::collections::VecDeque;
use std::sync::Arc;

use super::super::test_support::TestApp;
use super::super::{FileMeta, Metadata};
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
fn touching_preview_moves_it_to_the_lru_tail() {
    let mut app = TestApp::new();
    let first = Arc::<str>::from("first.png");
    let second = Arc::<str>::from("second.png");
    app.image_cache_order.push_back(first.clone());
    app.image_cache_order.push_back(second.clone());
    super::touch_order(&mut app.image_cache_order, &first);

    assert_eq!(app.image_cache_order, VecDeque::from([second, first]));
}
