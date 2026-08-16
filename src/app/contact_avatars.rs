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

pub const AVATAR_CACHE_CAPACITY: usize = 32;
pub const AVATAR_OVERSCAN: usize = 3;
pub const AVATAR_WORKERS: usize = 2;
const MAX_PERSISTED_BYTES: usize = 512 * 1024;
const RETRY_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvatarRequest {
    pub generation: u64,
    pub jid: wr::JID,
    pub refresh: bool,
}

pub enum AvatarResult {
    Cached {
        generation: u64,
        jid: wr::JID,
        picture_id: Arc<str>,
        protocol: StatefulProtocol,
    },
    Available {
        generation: u64,
        jid: wr::JID,
        picture_id: Arc<str>,
        protocol: StatefulProtocol,
    },
    Unavailable {
        generation: u64,
        jid: wr::JID,
    },
    Failed {
        generation: u64,
        jid: wr::JID,
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

    pub(crate) fn jid(&self) -> &wr::JID {
        match self {
            Self::Cached { jid, .. }
            | Self::Available { jid, .. }
            | Self::Unavailable { jid, .. }
            | Self::Failed { jid, .. } => jid,
        }
    }
}

pub fn prioritized_avatar_requests(
    chats: &[wr::JID],
    selected: Option<usize>,
    offset: usize,
    visible_count: usize,
) -> Vec<wr::JID> {
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

    pub fn picture_path(&self, jid: &wr::JID, picture_id: &str) -> PathBuf {
        self.root.join(format!(
            "{}-{}.image",
            safe_key(jid.0.as_ref()),
            safe_key(picture_id)
        ))
    }

    fn current_path(&self, jid: &wr::JID) -> PathBuf {
        self.root
            .join(format!("{}.current", safe_key(jid.0.as_ref())))
    }

    pub fn load_current(&self, jid: &wr::JID) -> io::Result<Option<(Arc<str>, Vec<u8>)>> {
        let current = self.current_path(jid);
        let picture_id = match fs::read_to_string(&current) {
            Ok(value) if value.len() <= 1024 && !value.is_empty() => value,
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let path = self.picture_path(jid, &picture_id);
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

    pub fn store(&self, jid: &wr::JID, picture_id: &str, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() || bytes.len() > MAX_PERSISTED_BYTES || picture_id.len() > 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid avatar cache entry",
            ));
        }
        atomic_write(&self.picture_path(jid, picture_id), bytes)?;
        atomic_write(&self.current_path(jid), picture_id.as_bytes())
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
    desired: Arc<Mutex<(u64, HashSet<wr::JID>)>>,
}

pub struct ContactAvatars {
    disk_root: PathBuf,
    runtime: Option<AvatarRuntime>,
    generation: u64,
    requested: Vec<wr::JID>,
    in_flight: HashSet<wr::JID>,
    unavailable: HashSet<wr::JID>,
    refreshed: HashSet<wr::JID>,
    failures: HashMap<wr::JID, (u64, Instant)>,
    protocols: HashMap<wr::JID, (Arc<str>, StatefulProtocol)>,
    order: VecDeque<wr::JID>,
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
        requests: Vec<wr::JID>,
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
        self.in_flight.retain(|jid| desired.contains(jid));
        let runtime = self
            .runtime
            .get_or_insert_with(|| start_runtime(self.disk_root.clone(), tx, picker));
        *runtime.desired.lock().unwrap() = (self.generation, desired);
        let now = Instant::now();
        for jid in &self.requested {
            let retryable = self
                .failures
                .get(jid)
                .is_none_or(|(failed_generation, retry_at)| {
                    *failed_generation != self.generation && now >= *retry_at
                });
            if (!self.protocols.contains_key(jid) || !self.refreshed.contains(jid))
                && !self.in_flight.contains(jid)
                && !self.unavailable.contains(jid)
                && retryable
            {
                let has_protocol = self.protocols.contains_key(jid);
                self.in_flight.insert(jid.clone());
                let _ = runtime.sender.send(AvatarRequest {
                    generation: self.generation,
                    jid: jid.clone(),
                    refresh: !has_protocol || !self.refreshed.contains(jid),
                });
            }
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

    pub fn apply(&mut self, result: AvatarResult) -> bool {
        let jid = result.jid().clone();
        self.in_flight.remove(&jid);
        if result.generation() != self.generation || !self.requested.contains(&jid) {
            return false;
        }
        match result {
            AvatarResult::Cached {
                picture_id,
                protocol,
                ..
            } => {
                self.insert(jid, picture_id, protocol);
            }
            AvatarResult::Available {
                picture_id,
                protocol,
                ..
            } => {
                let changed = self
                    .protocols
                    .get(&jid)
                    .is_none_or(|(id, _)| id != &picture_id);
                if changed {
                    self.insert(jid.clone(), picture_id, protocol);
                }
                self.refreshed.insert(jid);
            }
            AvatarResult::Unavailable { .. } => {
                self.protocols.remove(&jid);
                self.order.retain(|cached| cached != &jid);
                self.unavailable.insert(jid.clone());
                self.refreshed.insert(jid);
            }
            AvatarResult::Failed { generation, .. } => {
                self.failures
                    .insert(jid, (generation, Instant::now() + RETRY_COOLDOWN));
            }
        }
        true
    }

    pub fn mark_refreshed(&mut self, generation: u64, jid: wr::JID) -> bool {
        self.in_flight.remove(&jid);
        if generation != self.generation || !self.requested.contains(&jid) {
            return false;
        }
        self.refreshed.insert(jid);
        true
    }

    pub fn protocol_mut(&mut self, jid: &wr::JID) -> Option<&mut StatefulProtocol> {
        if self.protocols.contains_key(jid) {
            self.order.retain(|cached| cached != jid);
            self.order.push_back(jid.clone());
        }
        self.protocols.get_mut(jid).map(|(_, protocol)| protocol)
    }

    fn insert(&mut self, jid: wr::JID, picture_id: Arc<str>, protocol: StatefulProtocol) {
        if !self.protocols.contains_key(&jid)
            && self.protocols.len() >= AVATAR_CACHE_CAPACITY
            && let Some(oldest) = self.order.pop_front()
        {
            self.protocols.remove(&oldest);
        }
        self.order.retain(|cached| cached != &jid);
        self.order.push_back(jid.clone());
        self.protocols.insert(jid, (picture_id, protocol));
    }
}

fn start_runtime(
    disk_root: PathBuf,
    app_tx: mpsc::Sender<AppInput>,
    picker: Arc<Mutex<Picker>>,
) -> AvatarRuntime {
    let (sender, receiver) = mpsc::channel::<AvatarRequest>();
    let receiver = Arc::new(Mutex::new(receiver));
    let desired = Arc::new(Mutex::new((0, HashSet::new())));
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
                let current = wanted.0 == request.generation && wanted.1.contains(&request.jid);
                drop(wanted);
                if !current {
                    continue;
                }
                let cache = AvatarDiskCache::new(&disk_root).ok();
                let cached = cache
                    .as_ref()
                    .and_then(|cache| cache.load_current(&request.jid).ok().flatten())
                    .and_then(|(id, bytes)| {
                        decode_protocol(&picker, &bytes).map(|protocol| (id, protocol))
                    });
                let cached_id = cached.as_ref().map(|(id, _)| id.clone());
                if let Some((picture_id, protocol)) = cached {
                    let _ = app_tx.send(AppInput::App(AppEvent::ContactAvatar(
                        AvatarResult::Cached {
                            generation: request.generation,
                            jid: request.jid.clone(),
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
                            jid: request.jid,
                        },
                    )));
                    continue;
                }
                let result = match wr::get_profile_picture(&request.jid) {
                    Ok(wr::ProfilePictureAvailability::Available(picture)) => {
                        let unchanged = cached_id.as_ref().is_some_and(|id| id == &picture.id);
                        if unchanged {
                            None
                        } else {
                            if let Some(cache) = &cache {
                                let _ = cache.store(&request.jid, &picture.id, &picture.bytes);
                            }
                            match decode_protocol(&picker, &picture.bytes) {
                                Some(protocol) => Some(AvatarResult::Available {
                                    generation: request.generation,
                                    jid: request.jid.clone(),
                                    picture_id: picture.id,
                                    protocol,
                                }),
                                None => Some(AvatarResult::Failed {
                                    generation: request.generation,
                                    jid: request.jid.clone(),
                                }),
                            }
                        }
                    }
                    Ok(wr::ProfilePictureAvailability::Unavailable) => {
                        Some(AvatarResult::Unavailable {
                            generation: request.generation,
                            jid: request.jid.clone(),
                        })
                    }
                    Err(_) => Some(AvatarResult::Failed {
                        generation: request.generation,
                        jid: request.jid.clone(),
                    }),
                };
                if let Some(result) = result {
                    let _ = app_tx.send(AppInput::App(AppEvent::ContactAvatar(result)));
                } else {
                    let _ = app_tx.send(AppInput::App(AppEvent::ContactAvatarRefreshed {
                        generation: request.generation,
                        jid: request.jid,
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
    use super::*;
    use tempfile::tempdir;

    fn jid(index: usize) -> wr::JID {
        wr::JID::from(format!("contact-{index}@s.whatsapp.net"))
    }

    fn protocol() -> StatefulProtocol {
        let picker = Picker::halfblocks();
        picker.new_resize_protocol(image::DynamicImage::new_rgb8(1, 1))
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
        avatars.requested = vec![jid(1), jid(2)];
        avatars.in_flight.extend([jid(1), jid(2)]);
        avatars.generation = 2;
        avatars.requested = vec![jid(8)];
        let requested = avatars.requested.iter().cloned().collect::<HashSet<_>>();
        avatars
            .in_flight
            .retain(|candidate| requested.contains(candidate));

        assert!(avatars.in_flight.is_empty());
        assert!(!avatars.apply(AvatarResult::Failed {
            generation: 1,
            jid: jid(1)
        }));
        assert!(!avatars.failures.contains_key(&jid(1)));
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
        avatars.requested = vec![jid(1), jid(2)];

        assert!(avatars.apply(AvatarResult::Unavailable {
            generation: 3,
            jid: jid(1)
        }));
        assert!(avatars.unavailable.contains(&jid(1)));
        assert!(avatars.apply(AvatarResult::Failed {
            generation: 3,
            jid: jid(2)
        }));
        assert!(avatars.failures[&jid(2)].1 > Instant::now());
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
        let contact = jid(1);
        avatars.generation = 1;
        avatars.requested = vec![contact.clone()];
        avatars.insert(contact.clone(), "old".into(), protocol());
        avatars.mark_refreshed(1, contact.clone());
        assert_eq!(avatars.protocols[&contact].0.as_ref(), "old");

        avatars.apply(AvatarResult::Available {
            generation: 1,
            jid: contact.clone(),
            picture_id: "new".into(),
            protocol: protocol(),
        });
        assert_eq!(avatars.protocols[&contact].0.as_ref(), "new");
    }

    #[test]
    fn stale_cached_avatar_is_ready_before_session_refresh_completes() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        let contact = jid(1);
        avatars.generation = 1;
        avatars.requested = vec![contact.clone()];

        assert!(avatars.apply(AvatarResult::Cached {
            generation: 1,
            jid: contact.clone(),
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
            avatars.insert(jid(index), format!("picture-{index}").into(), protocol());
        }
        assert_eq!(avatars.protocols.len(), AVATAR_CACHE_CAPACITY);
        assert!(!avatars.protocols.contains_key(&jid(0)));
    }

    #[test]
    fn clearing_window_invalidates_results_without_waiting_for_workers() {
        let root = tempdir().unwrap();
        let mut avatars = ContactAvatars::new(root.path().into());
        avatars.generation = 4;
        avatars.requested = vec![jid(1)];
        avatars.clear_window();
        assert!(avatars.requested.is_empty());
        assert!(!avatars.apply(AvatarResult::Failed {
            generation: 4,
            jid: jid(1)
        }));
    }
}
