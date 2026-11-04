use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use ratatui::layout::Size;
use ratatui_image::{Resize, ResizeEncodeRender, picker::Picker, protocol::StatefulProtocol};
use whatsrust as wr;

use crate::app::SharedAvatarQueryPort;
use crate::app::events::{AppEvent, AppInput};

pub const AVATAR_CACHE_CAPACITY: usize = 32;
pub const AVATAR_OVERSCAN: usize = 3;
pub const AVATAR_WORKERS: usize = 2;
const RETRY_COOLDOWN: Duration = Duration::from_secs(30);

mod cache;
pub use cache::AvatarDiskCache;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AvatarTarget {
    Contact(wr::JID),
    CommunityRoot(wr::JID),
}

impl AvatarTarget {
    fn jid(&self) -> &wr::JID {
        match self {
            Self::Contact(jid) | Self::CommunityRoot(jid) => jid,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvatarRequest {
    pub generation: u64,
    pub target: AvatarTarget,
    pub refresh: bool,
}

pub enum AvatarResult {
    Cached {
        generation: u64,
        target: AvatarTarget,
        picture_id: Arc<str>,
        protocol: StatefulProtocol,
    },
    Available {
        generation: u64,
        target: AvatarTarget,
        picture_id: Arc<str>,
        protocol: StatefulProtocol,
    },
    Fallback {
        generation: u64,
        target: AvatarTarget,
        protocol: StatefulProtocol,
    },
    Unavailable {
        generation: u64,
        target: AvatarTarget,
    },
    Failed {
        generation: u64,
        target: AvatarTarget,
    },
}

impl AvatarResult {
    pub(crate) fn generation(&self) -> u64 {
        match self {
            Self::Cached { generation, .. }
            | Self::Available { generation, .. }
            | Self::Fallback { generation, .. }
            | Self::Unavailable { generation, .. }
            | Self::Failed { generation, .. } => *generation,
        }
    }

    pub(crate) fn target(&self) -> &AvatarTarget {
        match self {
            Self::Cached { target, .. }
            | Self::Available { target, .. }
            | Self::Fallback { target, .. }
            | Self::Unavailable { target, .. }
            | Self::Failed { target, .. } => target,
        }
    }
}

pub fn prioritized_avatar_requests<T: Clone + Eq>(
    chats: &[T],
    selected: Option<usize>,
    offset: usize,
    visible_count: usize,
) -> Vec<T> {
    if chats.is_empty() || visible_count == 0 {
        return Vec::new();
    }
    let visible_end = offset.saturating_add(visible_count).min(chats.len());
    let overscan_start = offset.saturating_sub(AVATAR_OVERSCAN);
    let overscan_end = visible_end.saturating_add(AVATAR_OVERSCAN).min(chats.len());
    let mut result = Vec::new();
    if let Some(index) = selected.filter(|index| *index < chats.len()) {
        result.push(chats[index].clone());
    }
    for chat in &chats[offset.min(chats.len())..visible_end] {
        if !result.contains(chat) {
            result.push(chat.clone());
        }
    }
    for chat in &chats[overscan_start..offset.min(chats.len())] {
        if !result.contains(chat) {
            result.push(chat.clone());
        }
    }
    for chat in &chats[visible_end..overscan_end] {
        if !result.contains(chat) {
            result.push(chat.clone());
        }
    }
    result
}

struct AvatarRuntime {
    sender: mpsc::Sender<AvatarRequest>,
    desired: Arc<Mutex<(u64, HashSet<AvatarTarget>)>>,
    workers: Vec<thread::JoinHandle<()>>,
}

impl AvatarRuntime {
    fn shutdown(self) {
        drop(self.sender);
        for worker in self.workers {
            let _ = worker.join();
        }
    }
}

fn cache_for_target(disk_root: &Path, target: &AvatarTarget) -> Option<AvatarDiskCache> {
    let root = match target {
        AvatarTarget::Contact(_) => disk_root.to_path_buf(),
        AvatarTarget::CommunityRoot(_) => disk_root.join("community-roots"),
    };
    AvatarDiskCache::new(root).ok()
}

pub struct ContactAvatars {
    disk_root: PathBuf,
    avatar_query: Option<SharedAvatarQueryPort>,
    runtime: Option<AvatarRuntime>,
    generation: u64,
    requested: Vec<AvatarTarget>,
    in_flight: HashSet<AvatarTarget>,
    unavailable: HashSet<AvatarTarget>,
    refreshed: HashSet<AvatarTarget>,
    failures: HashMap<AvatarTarget, (u64, Instant)>,
    protocols: HashMap<AvatarTarget, (Arc<str>, StatefulProtocol)>,
    order: VecDeque<AvatarTarget>,
    #[cfg(test)]
    enqueued: usize,
}

impl Drop for ContactAvatars {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl ContactAvatars {
    pub fn new(disk_root: PathBuf) -> Self {
        Self {
            disk_root,
            avatar_query: None,
            runtime: None,
            generation: 0,
            requested: Vec::new(),
            in_flight: HashSet::new(),
            unavailable: HashSet::new(),
            refreshed: HashSet::new(),
            failures: HashMap::new(),
            protocols: HashMap::new(),
            order: VecDeque::new(),
            #[cfg(test)]
            enqueued: 0,
        }
    }

    pub(crate) fn with_avatar_query(
        disk_root: PathBuf,
        avatar_query: SharedAvatarQueryPort,
    ) -> Self {
        let mut avatars = Self::new(disk_root);
        avatars.avatar_query = Some(avatar_query);
        avatars
    }

    #[cfg(test)]
    pub(crate) fn set_avatar_query(&mut self, avatar_query: SharedAvatarQueryPort) {
        assert!(
            self.runtime.is_none(),
            "avatar query must be injected before scheduling"
        );
        self.avatar_query = Some(avatar_query);
    }

    pub fn schedule(
        &mut self,
        requests: Vec<AvatarTarget>,
        tx: mpsc::Sender<AppInput>,
        picker: Arc<Mutex<Picker>>,
    ) {
        if requests == self.requested {
            return;
        }
        if requests.is_empty() {
            self.clear_window();
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.requested = requests;
        let desired = self.requested.iter().cloned().collect::<HashSet<_>>();
        // Requests from the previous generation may be discarded by workers
        // before producing a result. Reset ownership so overlapping targets
        // are eligible for the current generation's request.
        self.in_flight.clear();
        let runtime = self.runtime.get_or_insert_with(|| {
            start_runtime(
                self.disk_root.clone(),
                tx,
                picker,
                Arc::clone(
                    self.avatar_query
                        .as_ref()
                        .expect("avatar query must be configured before scheduling"),
                ),
            )
        });
        *runtime.desired.lock().unwrap() = (self.generation, desired);
        let now = Instant::now();
        for target in &self.requested {
            let retryable =
                self.failures
                    .get(target)
                    .is_none_or(|(failed_generation, retry_at)| {
                        *failed_generation != self.generation && now >= *retry_at
                    });
            if (!self.protocols.contains_key(target) || !self.refreshed.contains(target))
                && !self.in_flight.contains(target)
                && (!self.unavailable.contains(target)
                    || (is_user_avatar(target) && !self.protocols.contains_key(target)))
                && retryable
            {
                let has_protocol = self.protocols.contains_key(target);
                self.in_flight.insert(target.clone());
                let _ = runtime.sender.send(AvatarRequest {
                    generation: self.generation,
                    target: target.clone(),
                    refresh: !self.unavailable.contains(target)
                        && (!has_protocol || !self.refreshed.contains(target)),
                });
                #[cfg(test)]
                {
                    self.enqueued += 1;
                }
            }
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown();
        }
    }

    pub fn clear_window(&mut self) {
        if self.requested.is_empty() {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.requested.clear();
        self.in_flight.clear();
        if let Some(runtime) = &self.runtime {
            *runtime.desired.lock().unwrap() = (self.generation, HashSet::new());
        }
    }

    #[cfg(test)]
    pub(crate) fn requested_targets(&self) -> &[AvatarTarget] {
        &self.requested
    }

    pub fn apply(&mut self, result: AvatarResult) -> bool {
        let target = result.target().clone();
        if result.generation() != self.generation || !self.requested.contains(&target) {
            return false;
        }
        self.in_flight.remove(&target);
        match result {
            AvatarResult::Cached {
                picture_id,
                protocol,
                ..
            } => {
                self.insert(target, picture_id, protocol);
            }
            AvatarResult::Available {
                picture_id,
                protocol,
                ..
            } => {
                self.unavailable.remove(&target);
                let changed = self
                    .protocols
                    .get(&target)
                    .is_none_or(|(id, _)| id != &picture_id);
                if changed {
                    self.insert(target.clone(), picture_id, protocol);
                }
                self.refreshed.insert(target);
            }
            AvatarResult::Fallback { protocol, .. } => {
                self.insert(target.clone(), "default-portrait".into(), protocol);
                self.unavailable.insert(target.clone());
                self.refreshed.insert(target);
            }
            AvatarResult::Unavailable { .. } => {
                self.protocols.remove(&target);
                self.order.retain(|cached| cached != &target);
                self.unavailable.insert(target.clone());
                self.refreshed.insert(target);
            }
            AvatarResult::Failed { generation, .. } => {
                self.failures
                    .insert(target, (generation, Instant::now() + RETRY_COOLDOWN));
            }
        }
        true
    }

    pub fn mark_refreshed(&mut self, generation: u64, target: AvatarTarget) -> bool {
        if generation != self.generation || !self.requested.contains(&target) {
            return false;
        }
        self.in_flight.remove(&target);
        self.refreshed.insert(target);
        true
    }

    pub fn status_label(&self, target: &AvatarTarget) -> &'static str {
        if self.failures.contains_key(target) || self.unavailable.contains(target) {
            "Photo unavailable"
        } else {
            "Loading photo…"
        }
    }

    pub fn protocol_mut(&mut self, target: &AvatarTarget) -> Option<&mut StatefulProtocol> {
        if self.protocols.contains_key(target) {
            self.order.retain(|cached| cached != target);
            self.order.push_back(target.clone());
        }
        self.protocols.get_mut(target).map(|(_, protocol)| protocol)
    }

    fn insert(&mut self, target: AvatarTarget, picture_id: Arc<str>, protocol: StatefulProtocol) {
        if !self.protocols.contains_key(&target)
            && self.protocols.len() >= AVATAR_CACHE_CAPACITY
            && let Some(oldest) = self.order.pop_front()
        {
            self.protocols.remove(&oldest);
        }
        self.order.retain(|cached| cached != &target);
        self.order.push_back(target.clone());
        self.protocols.insert(target, (picture_id, protocol));
    }
}

fn start_runtime(
    disk_root: PathBuf,
    app_tx: mpsc::Sender<AppInput>,
    picker: Arc<Mutex<Picker>>,
    avatar_query: SharedAvatarQueryPort,
) -> AvatarRuntime {
    let (sender, receiver) = mpsc::channel::<AvatarRequest>();
    let receiver = Arc::new(Mutex::new(receiver));
    let desired = Arc::new(Mutex::new((0, HashSet::new())));
    let mut workers = Vec::with_capacity(AVATAR_WORKERS);
    for _ in 0..AVATAR_WORKERS {
        let receiver = Arc::clone(&receiver);
        let desired_worker = Arc::clone(&desired);
        let app_tx = app_tx.clone();
        let picker = Arc::clone(&picker);
        let disk_root = disk_root.clone();
        let avatar_query = Arc::clone(&avatar_query);
        workers.push(thread::spawn(move || {
            loop {
                let Ok(request) = receiver.lock().unwrap().recv() else {
                    return;
                };
                let wanted = desired_worker.lock().unwrap();
                let current = wanted.0 == request.generation && wanted.1.contains(&request.target);
                drop(wanted);
                if !current {
                    continue;
                }
                if !request.refresh && is_user_avatar(&request.target) {
                    let result =
                        default_portrait_result(&picker, request.generation, request.target);
                    let _ = app_tx.send(AppInput::App(AppEvent::ContactAvatar(result)));
                    continue;
                }
                let cache = cache_for_target(&disk_root, &request.target);
                let jid = request.target.jid();
                let cached = cache
                    .as_ref()
                    .and_then(|cache| cache.load_current(jid).ok().flatten())
                    .and_then(|(id, bytes)| {
                        decode_protocol(&picker, &bytes).map(|protocol| (id, protocol))
                    });
                let cached_id = cached.as_ref().map(|(id, _)| id.clone());
                if let Some((picture_id, protocol)) = cached {
                    let _ = app_tx.send(AppInput::App(AppEvent::ContactAvatar(
                        AvatarResult::Cached {
                            generation: request.generation,
                            target: request.target.clone(),
                            picture_id,
                            protocol,
                        },
                    )));
                }
                if !request.refresh {
                    if cached_id.is_some() {
                        continue;
                    }
                    let _ = app_tx.send(AppInput::App(AppEvent::ContactAvatar(
                        AvatarResult::Failed {
                            generation: request.generation,
                            target: request.target,
                        },
                    )));
                    continue;
                }
                let result = match match &request.target {
                    AvatarTarget::Contact(_) => avatar_query.get_profile_picture(jid),
                    AvatarTarget::CommunityRoot(_) => {
                        avatar_query.get_community_profile_picture(jid)
                    }
                } {
                    Ok(wr::ProfilePictureAvailability::Available(picture)) => {
                        let unchanged = cached_id.as_ref().is_some_and(|id| id == &picture.id);
                        if unchanged {
                            None
                        } else {
                            if let Some(cache) = &cache {
                                let _ = cache.store(jid, &picture.id, &picture.bytes);
                            }
                            match decode_protocol(&picker, &picture.bytes) {
                                Some(protocol) => Some(AvatarResult::Available {
                                    generation: request.generation,
                                    target: request.target.clone(),
                                    picture_id: picture.id,
                                    protocol,
                                }),
                                None => Some(AvatarResult::Failed {
                                    generation: request.generation,
                                    target: request.target.clone(),
                                }),
                            }
                        }
                    }
                    Ok(wr::ProfilePictureAvailability::Unavailable) => {
                        if is_user_avatar(&request.target) {
                            Some(default_portrait_result(
                                &picker,
                                request.generation,
                                request.target.clone(),
                            ))
                        } else {
                            Some(AvatarResult::Unavailable {
                                generation: request.generation,
                                target: request.target.clone(),
                            })
                        }
                    }
                    Err(_) => Some(AvatarResult::Failed {
                        generation: request.generation,
                        target: request.target.clone(),
                    }),
                };
                if let Some(result) = result {
                    let _ = app_tx.send(AppInput::App(AppEvent::ContactAvatar(result)));
                } else {
                    let _ = app_tx.send(AppInput::App(AppEvent::ContactAvatarRefreshed {
                        generation: request.generation,
                        target: request.target,
                    }));
                }
            }
        }));
    }
    AvatarRuntime {
        sender,
        desired,
        workers,
    }
}

fn is_user_avatar(target: &AvatarTarget) -> bool {
    matches!(target, AvatarTarget::Contact(jid)
        if jid.0.ends_with("@s.whatsapp.net") || jid.0.ends_with("@lid"))
}

fn default_portrait_result(
    picker: &Arc<Mutex<Picker>>,
    generation: u64,
    target: AvatarTarget,
) -> AvatarResult {
    match decode_protocol(
        picker,
        include_bytes!("../../assets/contact-avatar-default.png"),
    ) {
        Some(protocol) => AvatarResult::Fallback {
            generation,
            target,
            protocol,
        },
        None => AvatarResult::Failed { generation, target },
    }
}

fn decode_protocol(picker: &Arc<Mutex<Picker>>, bytes: &[u8]) -> Option<StatefulProtocol> {
    let image = image::load_from_memory(bytes).ok()?;
    let mut protocol = picker.lock().ok()?.new_resize_protocol(image);
    protocol.resize_encode(&Resize::Scale(None), Size::new(4, 2));
    Some(protocol)
}

#[cfg(test)]
mod tests {
    use super::super::{
        Chat, CommunityNode,
        test_support::{AvatarQueryCall, FakeAvatarQuery, TestApp},
    };
    use super::*;
    use crate::ui::communities;
    use ratatui::{Terminal, backend::TestBackend};
    use std::io;
    use tempfile::tempdir;

    fn jid(index: usize) -> wr::JID {
        wr::JID::from(format!("contact-{index}@s.whatsapp.net"))
    }

    fn target(index: usize) -> AvatarTarget {
        AvatarTarget::Contact(jid(index))
    }

    fn protocol() -> StatefulProtocol {
        let picker = Picker::halfblocks();
        picker.new_resize_protocol(image::DynamicImage::new_rgb8(1, 1))
    }

    #[test]
    fn repeated_communities_render_keeps_avatar_requests_and_initials_stable() {
        let mut test_app = TestApp::new();
        let root = wr::JID::from("root@g.us".to_owned());
        let group = wr::JID::from("group@g.us".to_owned());
        test_app.chats.insert(
            group.clone(),
            Chat {
                jid: group.clone(),
                last_message_time: None,
            },
        );
        test_app.communities = vec![
            CommunityNode {
                jid: root,
                name: "Community".into(),
                is_root: true,
                linked_groups: vec![group.clone()],
                is_joined: true,
                is_default_subgroup: false,
                is_announce: None,
                participant_count: None,
            },
            CommunityNode {
                jid: group,
                name: "Group".into(),
                is_root: false,
                linked_groups: Vec::new(),
                is_joined: true,
                is_default_subgroup: false,
                is_announce: Some(false),
                participant_count: None,
            },
        ];
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();

        terminal
            .draw(|frame| communities::render(frame, &mut test_app, frame.area()))
            .unwrap();
        let generation = test_app.contact_avatars.generation;
        let requested = test_app.contact_avatars.requested.clone();
        let first = terminal.backend().buffer().clone();

        terminal
            .draw(|frame| communities::render(frame, &mut test_app, frame.area()))
            .unwrap();
        let second = terminal.backend().buffer();

        assert_eq!(test_app.contact_avatars.generation, generation);
        assert_eq!(test_app.contact_avatars.requested, requested);
        assert_eq!(test_app.contact_avatars.enqueued, requested.len());
        assert_eq!(first[(1, 1)].symbol(), "C");
        assert_eq!(second[(1, 1)].symbol(), "C");
        assert_eq!(second[(1, 4)].symbol(), "G");
    }

    #[test]
    fn contact_and_community_root_same_jid_have_distinct_identities_and_cache_namespaces() {
        let root = tempdir().unwrap();
        let jid = wr::JID::from("same@g.us".to_owned());
        let contact = AvatarTarget::Contact(jid.clone());
        let community = AvatarTarget::CommunityRoot(jid);
        assert_ne!(contact, community);

        let contact_cache = cache_for_target(root.path(), &contact).unwrap();
        let community_cache = cache_for_target(root.path(), &community).unwrap();
        assert_ne!(contact_cache.root(), community_cache.root());
        assert_eq!(contact_cache.root(), root.path());
        assert_eq!(community_cache.root(), root.path().join("community-roots"));
    }

    #[test]
    fn visible_and_overscan_requests_are_selected_first_then_visible_then_edges() {
        let chats = (0..12).map(jid).collect::<Vec<_>>();
        assert_eq!(
            prioritized_avatar_requests(&chats, Some(5), 4, 3),
            vec![
                jid(5),
                jid(4),
                jid(6),
                jid(1),
                jid(2),
                jid(3),
                jid(7),
                jid(8),
                jid(9)
            ]
        );
    }

    #[test]
    fn scheduler_requeues_overlapping_target_after_generation_change() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        let (request_tx, _request_rx) = mpsc::channel::<AvatarRequest>();
        avatars.runtime = Some(AvatarRuntime {
            sender: request_tx,
            desired: Arc::new(Mutex::new((0, HashSet::new()))),
            workers: Vec::new(),
        });
        let (app_tx, _app_rx) = mpsc::channel();
        let picker = Arc::new(Mutex::new(Picker::halfblocks()));

        avatars.schedule(
            vec![target(1), target(2)],
            app_tx.clone(),
            Arc::clone(&picker),
        );
        assert_eq!(avatars.enqueued, 2);
        assert_eq!(avatars.in_flight, [target(1), target(2)].into());

        avatars.schedule(vec![target(1), target(8)], app_tx, picker);

        // The overlapping target was still in flight from the old generation,
        // but schedule must enqueue it again for the new generation.
        assert_eq!(avatars.enqueued, 4);
        assert_eq!(avatars.in_flight, [target(1), target(8)].into());
    }

    #[test]
    fn contact_and_community_targets_use_distinct_port_operations() {
        let root = tempdir().unwrap();
        let jid = wr::JID::from("same@g.us".to_owned());
        let contact = AvatarTarget::Contact(jid.clone());
        let community = AvatarTarget::CommunityRoot(jid.clone());
        let query = Arc::new(FakeAvatarQuery::default());
        query
            .contact_results
            .lock()
            .unwrap()
            .push_back(Ok(wr::ProfilePictureAvailability::Unavailable));
        query
            .community_results
            .lock()
            .unwrap()
            .push_back(Ok(wr::ProfilePictureAvailability::Unavailable));
        let (app_tx, app_rx) = mpsc::channel();
        let avatar_query: SharedAvatarQueryPort = query.clone();
        let runtime = start_runtime(
            root.path().into(),
            app_tx,
            Arc::new(Mutex::new(Picker::halfblocks())),
            avatar_query,
        );
        *runtime.desired.lock().unwrap() = (1, [contact.clone(), community.clone()].into());
        for target in [contact, community] {
            runtime
                .sender
                .send(AvatarRequest {
                    generation: 1,
                    target,
                    refresh: true,
                })
                .unwrap();
        }
        for _ in 0..2 {
            assert!(matches!(
                app_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
                AppInput::App(AppEvent::ContactAvatar(AvatarResult::Unavailable { .. }))
            ));
        }
        runtime.shutdown();
        let calls = query.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls
                .iter()
                .filter(|call| **call == AvatarQueryCall::Contact(jid.clone()))
                .count(),
            1
        );
        assert_eq!(
            calls
                .iter()
                .filter(|call| **call == AvatarQueryCall::CommunityRoot(jid.clone()))
                .count(),
            1
        );
    }

    #[test]
    fn missing_user_photo_renders_exact_portrait_and_real_photo_takes_precedence() {
        use crate::app::actions::{AppAction, FocusPane};
        use crate::app::events::MediaRenderPlan;
        use crate::app::read_receipts::VisibilityPlan;
        use crate::ui::contact_list::{AVATAR_HEIGHT, AVATAR_WIDTH, visible_contact_rows};
        use ratatui::{layout::Rect, style::Color};

        for available in [false, true] {
            let mut app = TestApp::new();
            let contact = wr::JID::from("15551234567@s.whatsapp.net".to_owned());
            app.apply_contact_refresh(vec![(contact.clone(), "Photo contact".into())]);
            app.open_chat_by_jid(contact.clone());
            app.focus_pane = FocusPane::Conversation;
            let query = Arc::new(FakeAvatarQuery::default());
            let picture = if available {
                let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                    480,
                    480,
                    image::Rgb([255, 0, 0]),
                ));
                let mut bytes = std::io::Cursor::new(Vec::new());
                image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
                wr::ProfilePictureAvailability::Available(wr::ProfilePicture {
                    id: "real-photo".into(),
                    picture_type: "image".into(),
                    bytes: bytes.into_inner(),
                })
            } else {
                wr::ProfilePictureAvailability::Unavailable
            };
            query.contact_results.lock().unwrap().push_back(Ok(picture));
            app.set_avatar_query(query);
            app.picker = Arc::new(Mutex::new(Picker::halfblocks()));
            app.dispatch_action(AppAction::OpenContactInfo);
            let mut terminal = Terminal::new(TestBackend::new(180, 50)).unwrap();
            let mut media = MediaRenderPlan::default();
            let mut visibility = VisibilityPlan::default();
            terminal
                .draw(|frame| {
                    crate::ui::draw_with_plan(frame, &mut app, &mut media, &mut visibility);
                })
                .unwrap();
            app.schedule_avatar_viewport(ratatui::layout::Rect::new(0, 0, 180, 50));
            let input = app.rx.recv_timeout(Duration::from_secs(2)).unwrap();
            match input {
                AppInput::App(event) => assert!(app.handle_avatar_event(event)),
                other => panic!("unexpected avatar input: {other:?}"),
            }
            let completed = terminal
                .draw(|frame| {
                    crate::ui::draw_with_plan(frame, &mut app, &mut media, &mut visibility);
                })
                .unwrap();
            // Compare like-for-like raw render buffers. Terminal diffing skips
            // trailing cells of wide Chafa glyphs, so TestBackend can retain old
            // invisible colors there even when the rendered image is correct.
            let buffer = completed.buffer;
            let contacts_area = app.mouse_hit_map.chat_list.expect("visible contacts body");
            let (rows, items) = app.cached_contact_view();
            let offset = app.chat_list_state.offset();
            let visible = visible_contact_rows(&items, offset, contacts_area.height);
            let (_, relative_y) = visible
                .into_iter()
                .find(|(index, _)| rows[*index].avatar_target() == Some(&contact))
                .expect("visible contact avatar row");
            let list_avatar = Rect::new(
                contacts_area.x,
                contacts_area.y + relative_y,
                AVATAR_WIDTH,
                AVATAR_HEIGHT,
            );
            let show_filters = crate::ui::show_chat_filter_header(&app);
            let (_, body) = crate::ui::chat_filter_areas(buffer.area, show_filters);
            let (_, details) =
                crate::ui::layout::contact_navigation_areas(body, app.pane_visibility, true);
            let portrait = crate::ui::contact_info::portrait_area(details.unwrap());
            assert_eq!((portrait.width, portrait.height), (38, 19));
            // Both the list avatar and the large portrait use the worker result.
            for area in [list_avatar, portrait] {
                let (left, right, top, bottom) = (area.x, area.right(), area.y, area.bottom());
                if available {
                    assert!(
                        (top..bottom).any(|y| (left..right).any(|x| {
                            let cell = &buffer[(x, y)];
                            cell.fg == Color::Rgb(255, 0, 0) || cell.bg == Color::Rgb(255, 0, 0)
                        })),
                        "real photo must replace the default at x={left}"
                    );
                } else {
                    let bytes = include_bytes!("../../assets/contact-avatar-default.png");
                    let portrait = image::load_from_memory(bytes).unwrap();
                    let mut reference = Picker::halfblocks().new_resize_protocol(portrait);
                    let mut expected = ratatui::buffer::Buffer::empty(area);
                    ratatui::widgets::StatefulWidget::render(
                        ratatui_image::StatefulImage::default(),
                        area,
                        &mut expected,
                        &mut reference,
                    );
                    for y in top..bottom {
                        for x in left..right {
                            assert_eq!(buffer[(x, y)].symbol(), expected[(x, y)].symbol());
                            assert_eq!(buffer[(x, y)].fg, expected[(x, y)].fg);
                            assert_eq!(buffer[(x, y)].bg, expected[(x, y)].bg);
                        }
                    }
                }
            }
            // A hidden contact list must not drop the open portrait request.
            app.pane_visibility.chat_list = false;
            app.schedule_avatar_viewport(ratatui::layout::Rect::new(0, 0, 80, 24));
            assert!(
                app.contact_avatars
                    .requested_targets()
                    .contains(&AvatarTarget::Contact(contact))
            );
        }
    }

    #[test]
    fn stale_portrait_fallback_cannot_replace_a_new_generation_photo() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        avatars.generation = 2;
        avatars.requested = vec![target(1)];
        assert!(avatars.apply(AvatarResult::Available {
            generation: 2,
            target: target(1),
            picture_id: "real".into(),
            protocol: protocol(),
        }));
        assert!(!avatars.apply(AvatarResult::Fallback {
            generation: 1,
            target: target(1),
            protocol: protocol(),
        }));
        assert_eq!(avatars.protocols[&target(1)].0.as_ref(), "real");
    }

    #[test]
    fn worker_count_is_bounded_to_two() {
        assert_eq!(AVATAR_WORKERS, 2);
    }

    #[test]
    fn unavailable_is_session_negative_and_transient_failure_has_cooldown() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        avatars.generation = 3;
        avatars.requested = vec![target(1), target(2)];

        assert!(avatars.apply(AvatarResult::Unavailable {
            generation: 3,
            target: target(1)
        }));
        assert!(avatars.unavailable.contains(&target(1)));
        assert!(avatars.apply(AvatarResult::Failed {
            generation: 3,
            target: target(2)
        }));
        assert!(avatars.failures[&target(2)].1 > Instant::now());
    }

    #[test]
    fn disk_keys_are_contained_and_atomic_replacement_updates_current_picture() {
        let root = tempdir().unwrap();
        let cache = AvatarDiskCache::new(root.path().join("avatars")).unwrap();
        let hostile = wr::JID::from("../../escape@s.whatsapp.net".to_owned());
        let first = vec![1, 2, 3];
        let second = vec![4, 5, 6];

        cache.store(&hostile, "../../first", &first).unwrap();
        let first_path = cache.picture_path(&hostile, "../../first");
        assert!(first_path.starts_with(cache.root()));
        assert_eq!(first_path.parent(), Some(cache.root()));
        cache.store(&hostile, "second", &second).unwrap();
        assert_eq!(cache.load_current(&hostile).unwrap().unwrap().1, second);
    }

    #[cfg(unix)]
    #[test]
    fn disk_cache_rejects_symlink_entries() {
        use std::os::unix::fs::symlink;

        let root = tempdir().unwrap();
        let cache = AvatarDiskCache::new(root.path().join("avatars")).unwrap();
        let contact = jid(1);
        std::fs::write(cache.current_path(&contact), "picture").unwrap();
        symlink(
            root.path().join("outside"),
            cache.picture_path(&contact, "picture"),
        )
        .unwrap();
        assert_eq!(
            cache.load_current(&contact).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn unchanged_picture_keeps_protocol_while_changed_picture_replaces_it() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        let contact = jid(1);
        avatars.generation = 1;
        let target = AvatarTarget::Contact(contact.clone());
        avatars.requested = vec![target.clone()];
        avatars.insert(target.clone(), "old".into(), protocol());
        avatars.mark_refreshed(1, target.clone());
        assert_eq!(avatars.protocols[&target].0.as_ref(), "old");

        avatars.apply(AvatarResult::Available {
            generation: 1,
            target: target.clone(),
            picture_id: "new".into(),
            protocol: protocol(),
        });
        assert_eq!(avatars.protocols[&target].0.as_ref(), "new");
    }

    #[test]
    fn stale_cached_avatar_is_ready_before_session_refresh_completes() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        let contact = jid(1);
        avatars.generation = 1;
        let target = AvatarTarget::Contact(contact.clone());
        avatars.requested = vec![target.clone()];

        assert!(avatars.apply(AvatarResult::Cached {
            generation: 1,
            target: target.clone(),
            picture_id: "cached".into(),
            protocol: protocol(),
        }));
        assert!(avatars.protocol_mut(&target).is_some());
        assert!(!avatars.refreshed.contains(&target));
    }

    #[test]
    fn memory_lru_evicts_at_32_without_touching_message_cache_state() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        for index in 0..=AVATAR_CACHE_CAPACITY {
            avatars.insert(target(index), format!("picture-{index}").into(), protocol());
        }
        assert_eq!(avatars.protocols.len(), AVATAR_CACHE_CAPACITY);
        assert!(!avatars.protocols.contains_key(&target(0)));
    }

    #[test]
    fn clearing_window_invalidates_results_without_waiting_for_workers() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        avatars.generation = 4;
        avatars.requested = vec![target(1)];
        avatars.clear_window();
        assert!(avatars.requested.is_empty());
        assert!(!avatars.apply(AvatarResult::Failed {
            generation: 4,
            target: target(1)
        }));
    }
}
