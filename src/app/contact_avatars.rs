use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use ratatui::layout::Size;
use ratatui_image::{Resize, ResizeEncodeRender, picker::Picker, protocol::StatefulProtocol};
use whatsrust as wr;

use crate::app::events::{AppEvent, AppInput};

pub use wr::ProfilePictureTarget as AvatarTarget;

pub const AVATAR_CACHE_CAPACITY: usize = 32;
pub const AVATAR_OVERSCAN: usize = 3;
pub const AVATAR_WORKERS: usize = 2;
const MAX_PERSISTED_BYTES: usize = 512 * 1024;
const RETRY_COOLDOWN: Duration = Duration::from_secs(30);

fn retry_eligible(failure: Option<&(u64, Instant)>, generation: u64, now: Instant) -> bool {
    failure.is_none_or(|(failure_generation, retry_at)| {
        *failure_generation != generation || now >= *retry_at
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvatarRequest {
    pub generation: u64,
    pub target: wr::ProfilePictureTarget,
    pub refresh: bool,
}

pub enum AvatarResult {
    Cached {
        generation: u64,
        target: wr::ProfilePictureTarget,
        picture_id: Arc<str>,
        protocol: StatefulProtocol,
    },
    Available {
        generation: u64,
        target: wr::ProfilePictureTarget,
        picture_id: Arc<str>,
        protocol: StatefulProtocol,
    },
    Unavailable {
        generation: u64,
        target: wr::ProfilePictureTarget,
    },
    Failed {
        generation: u64,
        target: wr::ProfilePictureTarget,
    },
}

impl AvatarResult {
    pub(crate) fn generation(&self) -> u64 {
        match self {
            Self::Cached { generation, .. }
            | Self::Available { generation, .. }
            | Self::Unavailable { generation, .. }
            | Self::Failed { generation, .. } => *generation,
        }
    }

    pub(crate) fn target(&self) -> &wr::ProfilePictureTarget {
        match self {
            Self::Cached { target, .. }
            | Self::Available { target, .. }
            | Self::Unavailable { target, .. }
            | Self::Failed { target, .. } => target,
        }
    }
}

pub fn prioritized_avatar_requests<T: Clone + PartialEq>(
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

#[derive(Debug)]
pub struct AvatarDiskCache {
    root: PathBuf,
}

impl AvatarDiskCache {
    pub fn new(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        if fs::symlink_metadata(&root)?.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "avatar cache root is a symlink",
            ));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn picture_path(&self, target: &wr::ProfilePictureTarget, picture_id: &str) -> PathBuf {
        self.root.join(format!(
            "{}-{}.image",
            safe_key(&format!("{target:?}")),
            safe_key(picture_id)
        ))
    }

    fn current_path(&self, target: &wr::ProfilePictureTarget) -> PathBuf {
        self.root
            .join(format!("{}.current", safe_key(&format!("{target:?}"))))
    }

    pub fn load_current(
        &self,
        target: &wr::ProfilePictureTarget,
    ) -> io::Result<Option<(Arc<str>, Vec<u8>)>> {
        let current = self.current_path(target);
        let picture_id = match fs::read_to_string(&current) {
            Ok(value) if value.len() <= 1024 && !value.is_empty() => value,
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let path = self.picture_path(target, &picture_id);
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "avatar cache entry is a symlink",
            ));
        }
        let bytes = fs::read(path)?;
        if bytes.is_empty() || bytes.len() > MAX_PERSISTED_BYTES {
            return Ok(None);
        }
        Ok(Some((picture_id.into(), bytes)))
    }

    pub fn store(
        &self,
        target: &wr::ProfilePictureTarget,
        picture_id: &str,
        bytes: &[u8],
    ) -> io::Result<()> {
        if bytes.is_empty() || bytes.len() > MAX_PERSISTED_BYTES || picture_id.len() > 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid avatar cache entry",
            ));
        }
        atomic_write(&self.picture_path(target, picture_id), bytes)?;
        atomic_write(&self.current_path(target), picture_id.as_bytes())
    }
}

fn safe_key(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(&temporary) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(&temporary)?;
            options.open(&temporary)?
        }
        Err(error) => return Err(error),
    };
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)
}

struct AvatarRuntime {
    sender: mpsc::Sender<AvatarRequest>,
    desired: Arc<Mutex<(u64, HashSet<wr::ProfilePictureTarget>)>>,
}

pub struct ContactAvatars {
    disk_root: PathBuf,
    runtime: Option<AvatarRuntime>,
    generation: u64,
    requested: Vec<wr::ProfilePictureTarget>,
    in_flight: HashSet<wr::ProfilePictureTarget>,
    unavailable: HashSet<wr::ProfilePictureTarget>,
    refreshed: HashSet<wr::ProfilePictureTarget>,
    failures: HashMap<wr::ProfilePictureTarget, (u64, Instant)>,
    protocols: HashMap<wr::ProfilePictureTarget, (Arc<str>, StatefulProtocol)>,
    order: VecDeque<wr::ProfilePictureTarget>,
}

impl ContactAvatars {
    pub fn new(disk_root: PathBuf) -> Self {
        Self {
            disk_root,
            runtime: None,
            generation: 0,
            requested: Vec::new(),
            in_flight: HashSet::new(),
            unavailable: HashSet::new(),
            refreshed: HashSet::new(),
            failures: HashMap::new(),
            protocols: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn schedule(
        &mut self,
        requests: Vec<wr::ProfilePictureTarget>,
        tx: mpsc::Sender<AppInput>,
        picker: Arc<Mutex<Picker>>,
    ) {
        let changed = requests != self.requested;
        if requests.is_empty() {
            self.clear_window();
            return;
        }
        if changed {
            self.generation = self.generation.wrapping_add(1);
            self.requested = requests;
            self.in_flight.clear();
        }
        let desired = self.requested.iter().cloned().collect::<HashSet<_>>();
        let requests = self.take_requests(Instant::now());
        let runtime = self
            .runtime
            .get_or_insert_with(|| start_runtime(self.disk_root.clone(), tx, picker));
        *runtime.desired.lock().unwrap() = (self.generation, desired);
        for request in requests {
            let _ = runtime.sender.send(request);
        }
    }

    fn take_requests(&mut self, now: Instant) -> Vec<AvatarRequest> {
        let mut requests = Vec::new();
        for target in self.requested.clone() {
            let retryable = retry_eligible(self.failures.get(&target), self.generation, now);
            if (!self.protocols.contains_key(&target) || !self.refreshed.contains(&target))
                && !self.in_flight.contains(&target)
                && !self.unavailable.contains(&target)
                && retryable
            {
                let has_protocol = self.protocols.contains_key(&target);
                self.in_flight.insert(target.clone());
                requests.push(AvatarRequest {
                    generation: self.generation,
                    target: target.clone(),
                    refresh: !has_protocol || !self.refreshed.contains(&target),
                });
            }
        }
        requests
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

    pub fn apply(&mut self, result: AvatarResult) -> bool {
        let target = result.target().clone();
        if result.generation() != self.generation || !self.requested.contains(&target) {
            return false;
        }
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
                self.in_flight.remove(&target);
                let changed = self
                    .protocols
                    .get(&target)
                    .is_none_or(|(id, _)| id != &picture_id);
                if changed {
                    self.insert(target.clone(), picture_id, protocol);
                }
                self.refreshed.insert(target);
            }
            AvatarResult::Unavailable { .. } => {
                self.in_flight.remove(&target);
                self.protocols.remove(&target);
                self.order.retain(|cached| cached != &target);
                self.unavailable.insert(target.clone());
                self.refreshed.insert(target);
            }
            AvatarResult::Failed { generation, .. } => {
                self.in_flight.remove(&target);
                self.failures
                    .insert(target, (generation, Instant::now() + RETRY_COOLDOWN));
            }
        }
        true
    }

    pub fn mark_refreshed(&mut self, generation: u64, target: wr::ProfilePictureTarget) -> bool {
        if generation != self.generation || !self.requested.contains(&target) {
            return false;
        }
        self.in_flight.remove(&target);
        self.refreshed.insert(target);
        true
    }

    pub fn protocol_mut(
        &mut self,
        target: &wr::ProfilePictureTarget,
    ) -> Option<&mut StatefulProtocol> {
        if self.protocols.contains_key(target) {
            self.order.retain(|cached| cached != target);
            self.order.push_back(target.clone());
        }
        self.protocols.get_mut(target).map(|(_, protocol)| protocol)
    }

    fn insert(
        &mut self,
        target: wr::ProfilePictureTarget,
        picture_id: Arc<str>,
        protocol: StatefulProtocol,
    ) {
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
) -> AvatarRuntime {
    let (sender, receiver) = mpsc::channel::<AvatarRequest>();
    let receiver = Arc::new(Mutex::new(receiver));
    let desired = Arc::new(Mutex::new((0, HashSet::<wr::ProfilePictureTarget>::new())));
    for _ in 0..AVATAR_WORKERS {
        let receiver = Arc::clone(&receiver);
        let desired_worker = Arc::clone(&desired);
        let app_tx = app_tx.clone();
        let picker = Arc::clone(&picker);
        let disk_root = disk_root.clone();
        thread::spawn(move || {
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
                let cache = AvatarDiskCache::new(&disk_root).ok();
                let cached = cache
                    .as_ref()
                    .and_then(|cache| cache.load_current(&request.target).ok().flatten())
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
                let result = match wr::get_profile_picture_for_target(&request.target) {
                    Ok(wr::ProfilePictureAvailability::Available(picture)) => {
                        let unchanged = cached_id.as_ref().is_some_and(|id| id == &picture.id);
                        if unchanged {
                            None
                        } else {
                            if let Some(cache) = &cache {
                                let _ = cache.store(&request.target, &picture.id, &picture.bytes);
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
                        Some(AvatarResult::Unavailable {
                            generation: request.generation,
                            target: request.target.clone(),
                        })
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
        });
    }
    AvatarRuntime { sender, desired }
}

fn decode_protocol(picker: &Arc<Mutex<Picker>>, bytes: &[u8]) -> Option<StatefulProtocol> {
    let image = image::load_from_memory(bytes).ok()?;
    let mut protocol = picker.lock().ok()?.new_resize_protocol(image);
    protocol.resize_encode(&Resize::Scale(None), Size::new(4, 2));
    Some(protocol)
}

#[cfg(test)]
mod tests {
    use super::super::{Chat, CommunityNode, test_support::TestApp};
    use super::*;
    use crate::ui::communities;
    use ratatui::{Terminal, backend::TestBackend};
    use tempfile::tempdir;

    fn jid(index: usize) -> wr::JID {
        wr::JID::from(format!("contact-{index}@s.whatsapp.net"))
    }

    fn target(index: usize) -> wr::ProfilePictureTarget {
        wr::ProfilePictureTarget::Contact { jid: jid(index) }
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
        assert_eq!(first[(1, 1)].symbol(), "C");
        assert_eq!(second[(1, 1)].symbol(), "C");
        assert_eq!(second[(1, 4)].symbol(), "G");
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
    fn scheduler_deduplicates_in_flight_and_replaces_fast_scroll_generation() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        avatars.generation = 1;
        avatars.requested = vec![target(1), target(2)];
        avatars.in_flight.extend([target(1), target(2)]);
        avatars.generation = 2;
        avatars.requested = vec![target(8)];
        let requested = avatars.requested.iter().cloned().collect::<HashSet<_>>();
        avatars
            .in_flight
            .retain(|candidate| requested.contains(candidate));

        assert!(avatars.in_flight.is_empty());
        assert!(!avatars.apply(AvatarResult::Failed {
            generation: 1,
            target: target(1)
        }));
        assert!(!avatars.failures.contains_key(&target(1)));
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
    fn unchanged_window_failure_becomes_retry_eligible_after_cooldown() {
        let failed_at = Instant::now();
        let retry_at = failed_at + RETRY_COOLDOWN;
        let failure = (7, retry_at);

        assert!(!retry_eligible(
            Some(&failure),
            7,
            failed_at + RETRY_COOLDOWN - Duration::from_millis(1)
        ));
        assert!(retry_eligible(Some(&failure), 7, retry_at));
        assert!(retry_eligible(None, 7, failed_at));
    }

    #[test]
    fn disk_keys_are_contained_and_atomic_replacement_updates_current_picture() {
        let root = tempdir().unwrap();
        let cache = AvatarDiskCache::new(root.path().join("avatars")).unwrap();
        let first = vec![1, 2, 3];
        let second = vec![4, 5, 6];

        let hostile = target(99);
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
        let contact = target(1);
        fs::write(cache.current_path(&contact), "picture").unwrap();
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
        let contact = target(1);
        avatars.generation = 1;
        avatars.requested = vec![contact.clone()];
        avatars.insert(contact.clone(), "old".into(), protocol());
        avatars.mark_refreshed(1, contact.clone());
        assert_eq!(avatars.protocols[&contact].0.as_ref(), "old");

        avatars.apply(AvatarResult::Available {
            generation: 1,
            target: contact.clone(),
            picture_id: "new".into(),
            protocol: protocol(),
        });
        assert_eq!(avatars.protocols[&contact].0.as_ref(), "new");
    }

    #[test]
    fn cached_intermediate_result_does_not_reenqueue_refresh() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        let contact = target(1);
        avatars.generation = 1;
        avatars.requested = vec![contact.clone()];
        avatars.in_flight.insert(contact.clone());

        assert!(avatars.apply(AvatarResult::Cached {
            generation: 1,
            target: contact.clone(),
            picture_id: "cached".into(),
            protocol: protocol(),
        }));
        assert!(avatars.in_flight.contains(&contact));
        assert!(avatars.take_requests(Instant::now()).is_empty());
    }

    #[test]
    fn stale_generation_result_does_not_clear_current_in_flight() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        let contact = target(1);
        avatars.generation = 2;
        avatars.requested = vec![contact.clone()];
        avatars.in_flight.insert(contact.clone());

        assert!(!avatars.apply(AvatarResult::Available {
            generation: 1,
            target: contact.clone(),
            picture_id: "stale".into(),
            protocol: protocol(),
        }));
        assert!(avatars.in_flight.contains(&contact));
    }

    #[test]
    fn unchanged_window_failure_emits_no_request_before_cooldown() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        let contact = target(1);
        avatars.generation = 1;
        avatars.requested = vec![contact.clone()];
        avatars.in_flight.insert(contact.clone());
        avatars.apply(AvatarResult::Failed {
            generation: 1,
            target: contact.clone(),
        });

        let now = Instant::now();
        assert!(avatars.take_requests(now).is_empty());
        assert!(avatars.take_requests(now).is_empty());
    }

    #[test]
    fn unchanged_window_failure_enqueues_exactly_one_request_at_cooldown() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        let contact = target(1);
        let retry_at = Instant::now();
        avatars.generation = 1;
        avatars.requested = vec![contact.clone()];
        avatars.failures.insert(contact.clone(), (1, retry_at));

        assert_eq!(avatars.take_requests(retry_at).len(), 1);
        assert!(avatars.take_requests(retry_at).is_empty());
        assert!(avatars.in_flight.contains(&contact));
    }

    #[test]
    fn stale_cached_avatar_is_ready_before_session_refresh_completes() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        let contact = target(1);
        avatars.generation = 1;
        avatars.requested = vec![contact.clone()];

        assert!(avatars.apply(AvatarResult::Cached {
            generation: 1,
            target: contact.clone(),
            picture_id: "cached".into(),
            protocol: protocol(),
        }));
        assert!(avatars.protocol_mut(&contact).is_some());
        assert!(!avatars.refreshed.contains(&contact));
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
