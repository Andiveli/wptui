use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use super::{prepare_persisted_state, start_lifecycle};
use crate::app::lifecycle_control::{LifecycleControl, QrCallback};
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

#[derive(Debug, Eq, PartialEq)]
enum LifecycleCall {
    NewClient(String),
    CallbacksRegistered,
    ConnectRegistered,
    QrPresented(String),
    PairPhone(String),
    PairingPresented(String),
    Disconnect,
    Logout,
}

struct FakeLifecycleControl {
    calls: Arc<Mutex<Vec<LifecycleCall>>>,
    pairing_code: String,
    qr_callback: Mutex<Option<QrCallback>>,
}

impl FakeLifecycleControl {
    fn new(calls: Arc<Mutex<Vec<LifecycleCall>>>, pairing_code: impl Into<String>) -> Self {
        Self {
            calls,
            pairing_code: pairing_code.into(),
            qr_callback: Mutex::new(None),
        }
    }

    fn record(&self, call: LifecycleCall) {
        self.calls.lock().unwrap().push(call);
    }

    fn emit_qr(&self, qr: &str) {
        let mut callback = self
            .qr_callback
            .lock()
            .unwrap()
            .take()
            .expect("connect must register a QR callback");
        callback(qr.to_owned());
        self.qr_callback.lock().unwrap().replace(callback);
    }
}

impl LifecycleControl for FakeLifecycleControl {
    fn new_client(&self, database_path: &str) {
        self.record(LifecycleCall::NewClient(database_path.to_owned()));
    }

    fn connect(&self, callback: QrCallback) {
        self.record(LifecycleCall::ConnectRegistered);
        self.qr_callback.lock().unwrap().replace(callback);
    }

    fn pair_phone(&self, phone: &str) -> String {
        self.record(LifecycleCall::PairPhone(phone.to_owned()));
        self.pairing_code.clone()
    }

    fn disconnect(&self) {
        self.record(LifecycleCall::Disconnect);
    }

    fn logout(&self) {
        self.record(LifecycleCall::Logout);
    }
}

fn record(calls: &Arc<Mutex<Vec<LifecycleCall>>>, call: LifecycleCall) {
    calls.lock().unwrap().push(call);
}

#[test]
fn startup_creates_client_registers_callbacks_then_transfers_worker_and_connects() {
    let mut app = TestApp::new();
    let expected_database_path = app.whatsmeow_db.to_str().unwrap().to_owned();
    let calls = Arc::new(Mutex::new(vec![]));
    let lifecycle = Arc::new(FakeLifecycleControl::new(Arc::clone(&calls), "unused"));
    app.set_lifecycle_control(lifecycle);

    let registered_calls = Arc::clone(&calls);
    let qr_calls = Arc::clone(&calls);
    let pairing_calls = Arc::clone(&calls);
    let mut worker = start_lifecycle(
        &mut app,
        None,
        move || record(&registered_calls, LifecycleCall::CallbacksRegistered),
        move |qr| record(&qr_calls, LifecycleCall::QrPresented(qr)),
        move |code| record(&pairing_calls, LifecycleCall::PairingPresented(code)),
    );

    assert_eq!(
        calls.lock().unwrap().as_slice(),
        [
            LifecycleCall::NewClient(expected_database_path),
            LifecycleCall::CallbacksRegistered,
            LifecycleCall::ConnectRegistered,
        ]
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            app.take_media_download_worker()
        }))
        .is_err()
    );
    worker.shutdown();
}

#[test]
fn qr_callback_pairs_only_when_phone_is_requested() {
    struct Case {
        phone: Option<&'static str>,
        expected_calls: Vec<LifecycleCall>,
    }

    for case in [
        Case {
            phone: None,
            expected_calls: vec![
                LifecycleCall::NewClient("database path supplied by TestApp".into()),
                LifecycleCall::CallbacksRegistered,
                LifecycleCall::ConnectRegistered,
                LifecycleCall::QrPresented("qr payload".into()),
            ],
        },
        Case {
            phone: Some("15551234567"),
            expected_calls: vec![
                LifecycleCall::NewClient("database path supplied by TestApp".into()),
                LifecycleCall::CallbacksRegistered,
                LifecycleCall::ConnectRegistered,
                LifecycleCall::QrPresented("qr payload".into()),
                LifecycleCall::PairPhone("15551234567".into()),
                LifecycleCall::PairingPresented("pairing code".into()),
            ],
        },
    ] {
        let mut app = TestApp::new();
        let calls = Arc::new(Mutex::new(vec![]));
        let lifecycle = Arc::new(FakeLifecycleControl::new(
            Arc::clone(&calls),
            "pairing code",
        ));
        app.set_lifecycle_control(Arc::clone(&lifecycle));
        let expected_database_path = app.whatsmeow_db.to_str().unwrap().to_owned();

        let registered_calls = Arc::clone(&calls);
        let qr_calls = Arc::clone(&calls);
        let pairing_calls = Arc::clone(&calls);
        let mut worker = start_lifecycle(
            &mut app,
            case.phone.map(str::to_owned),
            move || record(&registered_calls, LifecycleCall::CallbacksRegistered),
            move |qr| record(&qr_calls, LifecycleCall::QrPresented(qr)),
            move |code| record(&pairing_calls, LifecycleCall::PairingPresented(code)),
        );
        lifecycle.emit_qr("qr payload");

        let expected_calls = case
            .expected_calls
            .into_iter()
            .map(|call| match call {
                LifecycleCall::NewClient(_) => {
                    LifecycleCall::NewClient(expected_database_path.clone())
                }
                call => call,
            })
            .collect::<Vec<_>>();
        assert_eq!(*calls.lock().unwrap(), expected_calls);
        worker.shutdown();
    }
}
