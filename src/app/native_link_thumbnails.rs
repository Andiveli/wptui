//! Stored-byte thumbnails only. Viewport demand is supplied after a successful draw.
use std::io::Cursor;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use image::DynamicImage;
use ratatui::layout::Size;
use ratatui_image::{Resize, ResizeEncodeRender, picker::Picker, protocol::StatefulProtocol};
use whatsrust as wr;

const MAX_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThumbnailKey {
    pub(crate) chat: wr::JID,
    pub(crate) message_id: wr::MessageId,
    pub(crate) source: Arc<str>,
    pub(crate) matched_url: Arc<str>,
    pub(crate) size: Size,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThumbnailTicket(u64);

#[derive(Clone)]
pub(crate) struct ThumbnailGuard(Arc<AtomicBool>);

impl ThumbnailGuard {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub(crate) struct ThumbnailJob {
    ticket: ThumbnailTicket,
    guard: ThumbnailGuard,
    bytes: Vec<u8>,
    size: Size,
}

impl ThumbnailJob {
    pub(crate) fn ticket(&self) -> ThumbnailTicket {
        self.ticket.clone()
    }
    pub(crate) fn guard(&self) -> ThumbnailGuard {
        self.guard.clone()
    }

    pub(crate) fn run_with<I, P>(
        &self,
        decode: impl FnOnce(&[u8]) -> Option<I>,
        prepare: impl FnOnce(I, Size) -> Option<P>,
    ) -> Option<P> {
        if self.guard.is_cancelled() {
            return None;
        }
        let image = decode(&self.bytes)?;
        if self.guard.is_cancelled() {
            return None;
        }
        prepare(image, self.size)
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum EntryState {
    Absent,
    Loading,
    Ready,
    Failed,
}

enum Value {
    Loading(ThumbnailTicket),
    Ready(StatefulProtocol),
    Failed,
}
struct Entry {
    key: ThumbnailKey,
    value: Value,
}
struct Outstanding {
    ticket: ThumbnailTicket,
    key: ThumbnailKey,
    guard: ThumbnailGuard,
}

pub(crate) struct NativeLinkThumbnails {
    // Oldest first. Small bounded linear lists avoid a second recency index.
    entries: Vec<Entry>,
    visible: Vec<ThumbnailKey>,
    jobs: Vec<Outstanding>,
    cache_capacity: usize,
    job_capacity: usize,
    generation: u64,
}

impl Default for NativeLinkThumbnails {
    fn default() -> Self {
        Self::new(64, 2)
    }
}

impl NativeLinkThumbnails {
    pub(crate) fn new(cache_capacity: usize, job_capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            visible: Vec::new(),
            jobs: Vec::new(),
            cache_capacity,
            job_capacity,
            generation: 0,
        }
    }

    pub(crate) fn reconcile(&mut self, visible: &[ThumbnailKey]) {
        self.visible = visible.to_vec();
        for job in &self.jobs {
            if !visible.contains(&job.key) {
                job.guard.cancel();
            }
        }
        self.entries.retain(|entry| {
            !matches!(&entry.value, Value::Loading(ticket)
            if self.jobs.iter().any(|job| &job.ticket == ticket && job.guard.is_cancelled()))
        });
    }

    pub(crate) fn admit(
        &mut self,
        key: &ThumbnailKey,
        supply: impl FnOnce() -> Option<Vec<u8>>,
    ) -> Option<ThumbnailJob> {
        if !self.visible.contains(key) {
            return None;
        }
        if let Some(index) = self.entries.iter().position(|entry| &entry.key == key) {
            let entry = self.entries.remove(index);
            self.entries.push(entry);
            return None;
        }
        if self.cache_capacity == 0 || self.jobs.len() >= self.job_capacity {
            return None;
        }
        let bytes = supply()?;
        let generation = self.generation.checked_add(1)?;
        self.generation = generation;
        if self.entries.len() == self.cache_capacity {
            let evicted = self.entries.remove(0);
            if let Value::Loading(ticket) = evicted.value {
                if let Some(job) = self.jobs.iter().find(|job| job.ticket == ticket) {
                    job.guard.cancel();
                }
            }
        }
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            self.entries.push(Entry {
                key: key.clone(),
                value: Value::Failed,
            });
            return None;
        }
        let ticket = ThumbnailTicket(generation);
        let guard = ThumbnailGuard(Arc::new(AtomicBool::new(false)));
        self.jobs.push(Outstanding {
            ticket: ticket.clone(),
            key: key.clone(),
            guard: guard.clone(),
        });
        self.entries.push(Entry {
            key: key.clone(),
            value: Value::Loading(ticket.clone()),
        });
        Some(ThumbnailJob {
            ticket,
            guard,
            bytes,
            size: key.size,
        })
    }

    pub(crate) fn pending_visible_keys(&self) -> impl Iterator<Item = &ThumbnailKey> {
        self.visible.iter().filter(|key| {
            self.cache_capacity > 0
                && self.jobs.len() < self.job_capacity
                && self.state(key) == EntryState::Absent
        })
    }

    pub(crate) fn key_for_ticket(&self, ticket: &ThumbnailTicket) -> Option<&ThumbnailKey> {
        self.jobs
            .iter()
            .find(|job| &job.ticket == ticket)
            .map(|job| &job.key)
    }

    pub(crate) fn cancel(&mut self, ticket: &ThumbnailTicket) {
        if let Some(job) = self.jobs.iter().find(|job| &job.ticket == ticket) {
            job.guard.cancel();
        }
    }

    pub(crate) fn finish(
        &mut self,
        ticket: ThumbnailTicket,
        protocol: Option<StatefulProtocol>,
    ) -> bool {
        let Some(index) = self.jobs.iter().position(|job| job.ticket == ticket) else {
            return false;
        };
        // Release exactly this ticket, even when its cache entry was evicted.
        let job = self.jobs.remove(index);
        let current = self
            .entries
            .iter()
            .position(|entry| matches!(&entry.value, Value::Loading(active) if active == &ticket));
        let Some(index) = current else {
            return false;
        };
        if job.guard.is_cancelled() || !self.visible.contains(&job.key) {
            self.entries.remove(index);
            return false;
        }
        self.entries[index].value = protocol.map_or(Value::Failed, Value::Ready);
        true
    }

    pub(crate) fn state(&self, key: &ThumbnailKey) -> EntryState {
        match self
            .entries
            .iter()
            .find(|entry| &entry.key == key)
            .map(|entry| &entry.value)
        {
            None => EntryState::Absent,
            Some(Value::Loading(_)) => EntryState::Loading,
            Some(Value::Ready(_)) => EntryState::Ready,
            Some(Value::Failed) => EntryState::Failed,
        }
    }

    pub(crate) fn protocol_mut(&mut self, key: &ThumbnailKey) -> Option<&mut StatefulProtocol> {
        let index = self.entries.iter().position(|entry| &entry.key == key)?;
        let entry = self.entries.remove(index);
        self.entries.push(entry);
        match &mut self.entries.last_mut()?.value {
            Value::Ready(protocol) => Some(protocol),
            _ => None,
        }
    }
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn outstanding(&self) -> usize {
        self.jobs.len()
    }
}

pub(crate) fn decode_thumbnail(bytes: &[u8]) -> Option<DynamicImage> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return None;
    }
    let bounded_reader = || {
        let mut reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()?;
        // Bound both header inspection and pixel decoding before invoking either.
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(2048);
        limits.max_image_height = Some(2048);
        limits.max_alloc = Some(16 * 1024 * 1024);
        reader.limits(limits);
        Some(reader)
    };
    let dimensions = bounded_reader()?.into_dimensions().ok()?;
    if dimensions.0 == 0 || dimensions.1 == 0 || dimensions.0 > 2048 || dimensions.1 > 2048 {
        return None;
    }
    let image = bounded_reader()?.decode().ok()?;
    Some(if image.width() > 512 || image.height() > 256 {
        image.thumbnail(512, 256)
    } else {
        image
    })
}

pub(crate) fn valid_size(size: Size) -> bool {
    size.width > 0 && size.height > 0 && size.width <= 128 && size.height <= 32
}

pub(crate) fn prepare_image(
    image: DynamicImage,
    size: Size,
    picker: &Arc<Mutex<Picker>>,
) -> Option<StatefulProtocol> {
    if !valid_size(size) {
        return None;
    }
    let picker = picker.lock().ok()?;
    let font = picker.font_size();
    // Cell geometry alone does not bound pixels on terminals with unusual fonts.
    let width = u32::from(size.width).checked_mul(u32::from(font.width))?;
    let height = u32::from(size.height).checked_mul(u32::from(font.height))?;
    if width == 0
        || height == 0
        || width > 2048
        || height > 2048
        || width.checked_mul(height)? > 1024 * 1024
    {
        return None;
    }
    let mut protocol = picker.new_resize_protocol(image);
    drop(picker);
    protocol.resize_encode(&Resize::Scale(None), size);
    Some(protocol)
}

pub(crate) fn prepare_thumbnail(
    bytes: &[u8],
    size: Size,
    picker: &Arc<Mutex<Picker>>,
) -> Option<StatefulProtocol> {
    if !valid_size(size) {
        return None;
    }
    prepare_image(decode_thumbnail(bytes)?, size, picker)
}
