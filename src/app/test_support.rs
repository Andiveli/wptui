use super::{
    App, AvatarQueryPort, ChatReadCursorPort, ChatSettingsQueryPort, Clock, CommunityQueryPort,
    ContactSourcePort, DmResolverPort, GroupInfoQueryPort, GroupParticipantsQueryPort,
    MessagePushNamePort, NotificationProjection, Notifier, PresenceSubscriptionPort,
    PurgeExpiredStatuses, PurgedExpiredStatuses, RawPresenceDiagnosticsPort, StatusCursorError,
    StatusCursorPort, StatusRetentionError, StatusRetentionPort, StoreChatReadCursor,
    StoreStatusCursor,
};
use crate::db::{
    DatabaseHandler, SqliteChatReadCursor, SqliteChatStoreHydration, SqliteContactWriter,
    SqliteMessageReactionWriter, SqliteStatusCursor, SqliteStatusRetention,
};
use std::path::Path;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};
use whatsrust as wr;
pub(crate) struct TestApp {
    pub(crate) app: App<'static>,
    _dir: tempfile::TempDir,
}

#[derive(Debug)]
pub(crate) struct FixedClock(pub(crate) Option<i64>);

impl FixedClock {
    pub(crate) fn new(value: i64) -> Self {
        Self(Some(value))
    }
}

impl Clock for FixedClock {
    fn unix_seconds(&self) -> Option<i64> {
        self.0
    }
}

#[derive(Clone)]
pub(crate) struct MutableClock(Arc<Mutex<Option<i64>>>);

impl MutableClock {
    pub(crate) fn new(value: Option<i64>) -> Self {
        Self(Arc::new(Mutex::new(value)))
    }

    pub(crate) fn set(&self, value: Option<i64>) {
        *self.0.lock().unwrap() = value;
    }
}

impl Clock for MutableClock {
    fn unix_seconds(&self) -> Option<i64> {
        *self.0.lock().unwrap()
    }
}

#[derive(Clone, Default)]
pub(crate) struct RecordingNotifier {
    pub(crate) notifications: Arc<Mutex<Vec<(String, String)>>>,
}

#[derive(Clone, Default)]
pub(crate) struct FakeContactSource {
    pub(crate) rows: Arc<Mutex<Vec<(wr::JID, Arc<str>)>>>,
    pub(crate) calls: Arc<Mutex<usize>>,
}

impl ContactSourcePort for FakeContactSource {
    fn get_contacts(&self) -> Vec<(wr::JID, Arc<str>)> {
        *self.calls.lock().unwrap() += 1;
        self.rows.lock().unwrap().clone()
    }
}

#[derive(Default)]
pub(crate) struct FakeMessagePushNamePort {
    names: HashMap<wr::MessageId, Arc<str>>,
}

impl FakeMessagePushNamePort {
    pub(crate) fn with_name(message_id: wr::MessageId, name: impl Into<Arc<str>>) -> Self {
        Self {
            names: HashMap::from([(message_id, name.into())]),
        }
    }
}

impl MessagePushNamePort for FakeMessagePushNamePort {
    fn lookup_push_name(&self, message_id: &wr::MessageId) -> Option<Arc<str>> {
        self.names.get(message_id).cloned()
    }
}

#[derive(Clone, Default)]
pub(crate) struct FakeDmResolver {
    pub(crate) result: Arc<Mutex<Option<wr::JID>>>,
    pub(crate) calls: Arc<Mutex<Vec<wr::JID>>>,
}

impl FakeDmResolver {
    pub(crate) fn with_result(result: Option<wr::JID>) -> Self {
        Self {
            result: Arc::new(Mutex::new(result)),
            calls: Default::default(),
        }
    }
}

impl DmResolverPort for FakeDmResolver {
    fn resolve_dm_chat(&self, sender: &wr::JID) -> Option<wr::JID> {
        self.calls.lock().unwrap().push(sender.clone());
        self.result.lock().unwrap().clone()
    }
}

#[derive(Clone, Default)]
pub(crate) struct FakeChatSettingsQuery {
    pub(crate) settings: Arc<Mutex<wr::ChatSettings>>,
    pub(crate) jids: Arc<Mutex<Vec<wr::JID>>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AvatarQueryCall {
    Contact(wr::JID),
    CommunityRoot(wr::JID),
}

#[derive(Clone, Default)]
pub(crate) struct FakeAvatarQuery {
    pub(crate) contact_results:
        Arc<Mutex<VecDeque<Result<wr::ProfilePictureAvailability, wr::ProfilePictureError>>>>,
    pub(crate) community_results:
        Arc<Mutex<VecDeque<Result<wr::ProfilePictureAvailability, wr::ProfilePictureError>>>>,
    pub(crate) calls: Arc<Mutex<Vec<AvatarQueryCall>>>,
}

impl AvatarQueryPort for FakeAvatarQuery {
    fn get_profile_picture(
        &self,
        jid: &wr::JID,
    ) -> Result<wr::ProfilePictureAvailability, wr::ProfilePictureError> {
        self.calls
            .lock()
            .unwrap()
            .push(AvatarQueryCall::Contact(jid.clone()));
        self.contact_results
            .lock()
            .unwrap()
            .pop_front()
            .expect("contact avatar result must be configured")
    }

    fn get_community_profile_picture(
        &self,
        jid: &wr::JID,
    ) -> Result<wr::ProfilePictureAvailability, wr::ProfilePictureError> {
        self.calls
            .lock()
            .unwrap()
            .push(AvatarQueryCall::CommunityRoot(jid.clone()));
        self.community_results
            .lock()
            .unwrap()
            .pop_front()
            .expect("community-root avatar result must be configured")
    }
}

struct UnavailableAvatarQuery;

impl AvatarQueryPort for UnavailableAvatarQuery {
    fn get_profile_picture(
        &self,
        _: &wr::JID,
    ) -> Result<wr::ProfilePictureAvailability, wr::ProfilePictureError> {
        Ok(wr::ProfilePictureAvailability::Unavailable)
    }

    fn get_community_profile_picture(
        &self,
        _: &wr::JID,
    ) -> Result<wr::ProfilePictureAvailability, wr::ProfilePictureError> {
        Ok(wr::ProfilePictureAvailability::Unavailable)
    }
}

impl ChatSettingsQueryPort for FakeChatSettingsQuery {
    fn get_chat_settings(&self, jid: &wr::JID) -> wr::ChatSettings {
        self.jids.lock().unwrap().push(jid.clone());
        self.settings.lock().unwrap().clone()
    }
}

pub(crate) type GroupQueryTrace = Arc<Mutex<Vec<&'static str>>>;

#[derive(Clone, Default)]
pub(crate) struct FakeGroupInfoQuery {
    pub(crate) results: Arc<Mutex<VecDeque<Result<wr::GroupInfo, wr::GroupInfoError>>>>,
    pub(crate) calls: Arc<Mutex<Vec<wr::JID>>>,
    pub(crate) trace: GroupQueryTrace,
}
impl GroupInfoQueryPort for FakeGroupInfoQuery {
    fn get_group_info(&self, jid: &wr::JID) -> Result<wr::GroupInfo, wr::GroupInfoError> {
        self.calls.lock().unwrap().push(jid.clone());
        self.trace.lock().unwrap().push("info");
        self.results
            .lock()
            .unwrap()
            .pop_front()
            .expect("group info result must be configured")
    }
}

#[derive(Clone, Default)]
pub(crate) struct FakeGroupParticipantsQuery {
    pub(crate) results: Arc<Mutex<VecDeque<Vec<wr::GroupParticipant>>>>,
    pub(crate) calls: Arc<Mutex<Vec<wr::JID>>>,
    pub(crate) trace: GroupQueryTrace,
}
impl GroupParticipantsQueryPort for FakeGroupParticipantsQuery {
    fn get_group_participants(&self, jid: &wr::JID) -> Vec<wr::GroupParticipant> {
        self.calls.lock().unwrap().push(jid.clone());
        self.trace.lock().unwrap().push("participants");
        self.results
            .lock()
            .unwrap()
            .pop_front()
            .expect("group participants must be configured")
    }
}

struct UnavailableGroupInfoQuery;
impl GroupInfoQueryPort for UnavailableGroupInfoQuery {
    fn get_group_info(&self, _: &wr::JID) -> Result<wr::GroupInfo, wr::GroupInfoError> {
        Err(wr::GroupInfoError::ClientUnavailable)
    }
}

struct EmptyGroupParticipantsQuery;
impl GroupParticipantsQueryPort for EmptyGroupParticipantsQuery {
    fn get_group_participants(&self, _: &wr::JID) -> Vec<wr::GroupParticipant> {
        Vec::new()
    }
}

#[derive(Clone, Default)]
pub(crate) struct FakeCommunityQuery {
    pub(crate) results: Arc<Mutex<VecDeque<Result<Vec<wr::CommunityInfo>, wr::CommunitiesError>>>>,
    pub(crate) calls: Arc<Mutex<usize>>,
}

impl CommunityQueryPort for FakeCommunityQuery {
    fn get_communities(&self) -> Result<Vec<wr::CommunityInfo>, wr::CommunitiesError> {
        *self.calls.lock().unwrap() += 1;
        self.results
            .lock()
            .unwrap()
            .pop_front()
            .expect("community result must be configured")
    }
}

#[derive(Clone, Default)]
pub(crate) struct FakeStatusCursorPort {
    pub(crate) loaded: Arc<Mutex<Vec<(wr::JID, i64)>>>,
    pub(crate) stored: Arc<Mutex<Vec<StoreStatusCursor>>>,
    pub(crate) fails: Arc<Mutex<bool>>,
}

impl StatusCursorPort for FakeStatusCursorPort {
    fn load(&self) -> Result<Vec<(wr::JID, i64)>, StatusCursorError> {
        Ok(self.loaded.lock().unwrap().clone())
    }

    fn store(&self, command: StoreStatusCursor) -> Result<(), StatusCursorError> {
        self.stored.lock().unwrap().push(command);
        if *self.fails.lock().unwrap() {
            Err(StatusCursorError("store failed".into()))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct FakeStatusRetentionPort {
    pub(crate) commands: Arc<Mutex<Vec<PurgeExpiredStatuses>>>,
    pub(crate) media_paths: Arc<Mutex<Vec<std::path::PathBuf>>>,
    pub(crate) error: Arc<Mutex<Option<Arc<str>>>>,
}

impl StatusRetentionPort for FakeStatusRetentionPort {
    fn purge_expired_statuses(
        &self,
        command: PurgeExpiredStatuses,
    ) -> Result<PurgedExpiredStatuses, StatusRetentionError> {
        self.commands.lock().unwrap().push(command);
        if let Some(error) = self.error.lock().unwrap().clone() {
            Err(StatusRetentionError(error))
        } else {
            Ok(PurgedExpiredStatuses {
                media_paths: self.media_paths.lock().unwrap().clone(),
            })
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct FakeChatReadCursorPort {
    pub(crate) loaded: Arc<Mutex<Vec<(wr::JID, wr::MessageId, i64)>>>,
    pub(crate) stored: Arc<Mutex<Vec<StoreChatReadCursor>>>,
    pub(crate) panic_on_store: Arc<Mutex<bool>>,
}

impl ChatReadCursorPort for FakeChatReadCursorPort {
    fn load(&self) -> Vec<(wr::JID, wr::MessageId, i64)> {
        self.loaded.lock().unwrap().clone()
    }

    fn store(&self, command: StoreChatReadCursor) {
        self.stored.lock().unwrap().push(command);
        assert!(
            !*self.panic_on_store.lock().unwrap(),
            "cursor storage failed"
        );
    }
}

impl Notifier for RecordingNotifier {
    fn show(&self, notification: &NotificationProjection) -> Result<(), String> {
        self.notifications
            .lock()
            .unwrap()
            .push((notification.summary.to_string(), notification.body.clone()));
        Ok(())
    }
}

struct AcceptedPresenceSubscription;

impl PresenceSubscriptionPort for AcceptedPresenceSubscription {
    fn subscribe(&self, _: &wr::JID) -> wr::SubscribePresenceResult {
        wr::SubscribePresenceResult::Accepted
    }
}

struct EmptyRawPresenceDiagnostics;

impl RawPresenceDiagnosticsPort for EmptyRawPresenceDiagnostics {
    fn drain(&self) -> Option<String> {
        None
    }
}

impl TestApp {
    pub(crate) fn new() -> Self {
        Self::with_presence_subscription(Box::new(AcceptedPresenceSubscription))
    }

    pub(crate) fn with_message_push_name(message_push_name: Box<dyn MessagePushNamePort>) -> Self {
        let mut test_app = Self::new();
        test_app.app.set_message_push_name(message_push_name);
        test_app
    }

    pub(crate) fn with_presence_subscription(
        presence_subscription: Box<dyn PresenceSubscriptionPort>,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::with_data_dir(dir.path(), dir.path());
        app.set_presence_subscription(presence_subscription);
        app.set_raw_presence_diagnostics(Box::new(EmptyRawPresenceDiagnostics));
        app.set_avatar_query(Arc::new(UnavailableAvatarQuery));
        app.set_contact_source(Box::new(FakeContactSource::default()));
        app.set_message_push_name(Box::new(FakeMessagePushNamePort::default()));
        app.set_chat_settings_query(Box::new(FakeChatSettingsQuery::default()));
        app.set_community_query(Box::new(FakeCommunityQuery::default()));
        app.set_dm_resolver(Box::new(FakeDmResolver::default()));
        app.set_group_info_query(Box::new(UnavailableGroupInfoQuery));
        app.set_group_participants_query(Box::new(EmptyGroupParticipantsQuery));
        app.db_handler.init();
        Self { app, _dir: dir }
    }

    pub(crate) fn with_database(path: &Path) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::with_data_dir(dir.path(), dir.path());
        app.set_presence_subscription(Box::new(AcceptedPresenceSubscription));
        app.set_raw_presence_diagnostics(Box::new(EmptyRawPresenceDiagnostics));
        app.set_avatar_query(Arc::new(UnavailableAvatarQuery));
        app.db_handler.init();
        let db_path = path.join("app.db");
        let db_handler = DatabaseHandler::new(&db_path);
        app.chat_store_write = Box::new(db_handler.chat_store_writer());
        app.set_contact_write(Box::new(SqliteContactWriter::new(&db_path)));
        app.set_message_reaction_write(Box::new(SqliteMessageReactionWriter::new(&db_path)));
        app.set_chat_read_cursor(Box::new(SqliteChatReadCursor::new(&db_path)));
        app.set_status_cursor(Box::new(SqliteStatusCursor::new(&db_path)));
        app.set_status_retention(Box::new(SqliteStatusRetention::new(&db_path)));
        std::mem::replace(&mut app.db_handler, db_handler).stop();
        app.chat_store_hydration = Box::new(SqliteChatStoreHydration::new(&db_path));
        app.set_contact_source(Box::new(FakeContactSource::default()));
        app.set_message_push_name(Box::new(FakeMessagePushNamePort::default()));
        app.set_chat_settings_query(Box::new(FakeChatSettingsQuery::default()));
        app.set_community_query(Box::new(FakeCommunityQuery::default()));
        app.set_dm_resolver(Box::new(FakeDmResolver::default()));
        app.set_group_info_query(Box::new(UnavailableGroupInfoQuery));
        app.set_group_participants_query(Box::new(EmptyGroupParticipantsQuery));
        app.db_handler.init();
        Self { app, _dir: dir }
    }

    pub(crate) fn with_ports<C, N>(clock: C, notifier: N) -> Self
    where
        C: Clock + 'static,
        N: Notifier + 'static,
    {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::with_data_dir_and_ports(
            dir.path(),
            dir.path(),
            Box::new(clock),
            Box::new(notifier),
        );
        app.set_presence_subscription(Box::new(AcceptedPresenceSubscription));
        app.set_raw_presence_diagnostics(Box::new(EmptyRawPresenceDiagnostics));
        app.set_avatar_query(Arc::new(UnavailableAvatarQuery));
        app.set_contact_source(Box::new(FakeContactSource::default()));
        app.set_message_push_name(Box::new(FakeMessagePushNamePort::default()));
        app.set_chat_settings_query(Box::new(FakeChatSettingsQuery::default()));
        app.set_community_query(Box::new(FakeCommunityQuery::default()));
        app.set_dm_resolver(Box::new(FakeDmResolver::default()));
        app.set_group_info_query(Box::new(UnavailableGroupInfoQuery));
        app.set_group_participants_query(Box::new(EmptyGroupParticipantsQuery));
        app.db_handler.init();
        Self { app, _dir: dir }
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        self.app.shutdown_avatar_runtime();
        self.app.db_handler.stop();
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

pub(crate) fn message(chat: &wr::JID, id: &str, timestamp: i64) -> wr::Message {
    wr::Message {
        info: wr::MessageInfo {
            id: id.into(),
            chat: chat.clone(),
            sender: chat.clone(),
            mentions_self: false,
            timestamp,
            forwarding: Default::default(),
            is_from_me: false,
            quote_id: None,
            read_by: 0,
        },
        message: wr::MessageContent::Text(id.into()),
    }
}
