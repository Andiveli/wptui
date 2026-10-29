use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use ratatui_image::protocol::StatefulProtocol;
use whatsrust as wr;

use super::media_jobs::MediaJobOwner;
use super::media_support::probe_audio_duration;
use super::{App, AppEvent, AppInput, FileMeta, Metadata};

const IMAGE_CACHE_CAPACITY: usize = 50;
const ANIMATED_CACHE_CAPACITY: usize = 4;

fn touch_order(order: &mut VecDeque<Arc<str>>, path: &Arc<str>) {
    order.retain(|cached| cached != path);
    order.push_back(path.clone());
}

impl App<'_> {
    pub fn touch_image_cache(&mut self, path: &Arc<str>) {
        if self.image_cache.contains_key(path) {
            touch_order(&mut self.image_cache_order, path);
        }
    }

    pub(crate) fn preview_result_is_current(&self, id: &wr::MessageId, path: &Arc<str>) -> bool {
        matches!(
            self.metadata.get(id),
            Some(Metadata::File(FileMeta::Loading))
        ) && matches!(self.messages.get(id).map(|m| &m.message),
                Some(wr::MessageContent::File(file)) if file.path == *path)
    }

    pub(crate) fn sticker_deadline(&self) -> Option<Instant> {
        self.visible_stickers
            .iter()
            .filter_map(|path| self.sticker_playback.get(path)?.deadline)
            .min()
    }

    pub(crate) fn advance_visible_stickers(&mut self, now: Instant) -> bool {
        for (path, playback) in &mut self.sticker_playback {
            if !self.visible_stickers.contains(path) {
                playback.deadline = None;
            }
        }
        let mut changed = false;
        for path in &self.visible_stickers {
            if let Some(playback) = self.sticker_playback.get_mut(path) {
                changed |= playback.advance(now);
            }
        }
        changed
    }

    pub(crate) fn cache_sticker_preview(
        &mut self,
        message_id: wr::MessageId,
        path: Arc<str>,
        playback: super::sticker_animation::StickerPlayback,
    ) {
        if !self.image_cache.contains_key(&path)
            && !self.sticker_playback.contains_key(&path)
            && self.image_cache.len() + self.sticker_playback.len() >= IMAGE_CACHE_CAPACITY
            && let Some(oldest) = self.image_cache_order.pop_front()
        {
            self.image_cache.remove(&oldest);
            self.sticker_playback.remove(&oldest);
            self.mark_evicted_preview_reloadable(&oldest);
        }
        if !self.sticker_playback.contains_key(&path)
            && self.sticker_playback.len() >= ANIMATED_CACHE_CAPACITY
            && let Some(oldest) = self
                .image_cache_order
                .iter()
                .find(|p| self.sticker_playback.contains_key(*p))
                .cloned()
        {
            self.sticker_playback.remove(&oldest);
            self.image_cache_order.retain(|p| p != &oldest);
            self.mark_evicted_preview_reloadable(&oldest);
        }
        self.sticker_playback.insert(path.clone(), playback);
        touch_order(&mut self.image_cache_order, &path);
        self.metadata
            .insert(message_id.clone(), Metadata::File(FileMeta::Loaded));
        self.message_height_cache.invalidate(&message_id);
    }

    pub(crate) fn touch_sticker_cache(&mut self, path: &Arc<str>) {
        if self.sticker_playback.contains_key(path) {
            touch_order(&mut self.image_cache_order, path);
        }
    }

    pub(crate) fn cache_file_preview(
        &mut self,
        message_id: wr::MessageId,
        file_path: Arc<str>,
        preview: StatefulProtocol,
    ) {
        if !self.image_cache.contains_key(&file_path)
            && !self.sticker_playback.contains_key(&file_path)
            && self.image_cache.len() + self.sticker_playback.len() >= IMAGE_CACHE_CAPACITY
            && let Some(oldest) = self.image_cache_order.pop_front()
        {
            self.image_cache.remove(&oldest);
            self.sticker_playback.remove(&oldest);
            self.mark_evicted_preview_reloadable(&oldest);
        }
        self.sticker_playback.remove(&file_path);
        self.image_cache.insert(file_path.clone(), preview);
        self.touch_image_cache(&file_path);
        self.metadata
            .insert(message_id.clone(), Metadata::File(FileMeta::Loaded));
        self.message_height_cache.invalidate(&message_id);
    }

    fn mark_evicted_preview_reloadable(&mut self, path: &Arc<str>) {
        for (id, message) in &self.messages {
            if matches!(&message.message, wr::MessageContent::File(file) if file.path == *path)
                && matches!(
                    self.metadata.get(id),
                    Some(Metadata::File(FileMeta::Loaded))
                )
            {
                self.metadata
                    .insert(id.clone(), Metadata::File(FileMeta::Downloaded));
                self.message_height_cache.invalidate(id);
            }
        }
    }

    /// Starts a managed background probe once an audio file is on disk. The job
    /// permit prevents a late result from reaching a shutting-down runtime.
    pub(crate) fn spawn_audio_duration_probe_if_missing(
        &self,
        message_id: &wr::MessageId,
        media_jobs: &mut MediaJobOwner,
    ) {
        let Some(file) = (match self
            .messages
            .get(message_id)
            .map(|message| &message.message)
        {
            Some(wr::MessageContent::File(file)) => Some(file.clone()),
            _ => None,
        }) else {
            return;
        };
        if !matches!(file.kind, wr::FileKind::Audio)
            || self.audio_durations.contains_key(file.path.as_ref())
        {
            return;
        }
        let tx = self.tx.clone();
        let media_path = self.media_path.to_owned();
        let message_id = message_id.clone();
        media_jobs.spawn(move |permit| {
            let absolute = media_path.join(file.path.as_ref());
            let duration = probe_audio_duration(&absolute);
            permit.send(
                &tx,
                AppInput::App(AppEvent::SetAudioDuration(message_id, file.path, duration)),
            );
        });
    }
}

#[cfg(test)]
mod tests;
