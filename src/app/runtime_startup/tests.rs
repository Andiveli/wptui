use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use super::prepare_persisted_state;
use crate::app::{
    PurgeExpiredStatuses,
    chat_store::hydration_port::{ChatStoreHydration, ChatStoreHydrationPort},
    test_support::{FakeStatusRetentionPort, TestApp},
};

struct MediaObservingHydration([PathBuf; 2], Arc<Mutex<Vec<[bool; 2]>>>);
impl ChatStoreHydrationPort for MediaObservingHydration {
    fn load(&self) -> ChatStoreHydration {
        self.1
            .lock()
            .unwrap()
            .push([self.0[0].exists(), self.0[1].exists()]);
        ChatStoreHydration {
            chats: vec![],
            contacts: vec![],
            messages: vec![],
            reactions: vec![],
        }
    }
}

#[test]
fn preparation_purges_owned_media_and_sidecar_before_hydration() {
    let mut app = TestApp::new();
    let media = app.media_path.join("videos/expired.mp4");
    let sidecar = app.media_path.join("videos/expired.jpg");
    std::fs::create_dir_all(media.parent().unwrap()).unwrap();
    for path in [&media, &sidecar] {
        std::fs::write(path, b"expired").unwrap();
    }
    let fake = FakeStatusRetentionPort::default();
    fake.media_paths
        .lock()
        .unwrap()
        .push(PathBuf::from("videos/expired.mp4"));
    let observations = Arc::new(Mutex::new(vec![]));
    app.status_retention = Box::new(fake.clone());
    app.set_chat_store_hydration(Box::new(MediaObservingHydration(
        [media.clone(), sidecar.clone()],
        observations.clone(),
    )));

    prepare_persisted_state(&mut app);

    assert!(
        matches!(fake.commands.lock().unwrap().as_slice(), [PurgeExpiredStatuses { now }] if *now > 0)
    );
    assert_eq!(*observations.lock().unwrap(), vec![[false, false]]);
    assert!(!media.exists() && !sidecar.exists());
}

#[test]
fn preparation_panics_before_media_deletion_or_hydration_when_retention_fails() {
    let mut app = TestApp::new();
    let media = app.media_path.join("images/expired.jpg");
    std::fs::create_dir_all(media.parent().unwrap()).unwrap();
    std::fs::write(&media, b"expired").unwrap();
    let fake = FakeStatusRetentionPort::default();
    *fake.error.lock().unwrap() = Some("retention failed".into());
    let observations = Arc::new(Mutex::new(vec![]));
    app.status_retention = Box::new(fake);
    app.set_chat_store_hydration(Box::new(MediaObservingHydration(
        [media.clone(), media.clone()],
        observations.clone(),
    )));

    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| prepare_persisted_state(
            &mut app
        )))
        .is_err()
    );
    assert!(media.exists());
    assert!(observations.lock().unwrap().is_empty());
}
