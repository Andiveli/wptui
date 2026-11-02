//! Small decoded thumbnails and exact-rectangle native payloads, never full media.
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use image::DynamicImage;
use ratatui::layout::{Rect, Size};
use ratatui_image::{FontSize, Resize, picker::Picker, protocol::Protocol};

// At most 50 sources (the shared image LRU): 25 MiB of retained RGBA pixels.
pub(crate) const SOURCE_WIDTH: u32 = 512;
pub(crate) const SOURCE_HEIGHT: u32 = 256;
const SOURCE_CAPACITY: usize = 50;
// Ready, failed, queued and preparing keys all count against the same bound.
const CROP_CAPACITY: usize = 64;
const WORKERS: usize = 4;
const BATCH_SIZE: usize = 4;
// Each fitted/cropped RGBA buffer is at most 1 MiB (they can coexist; protocol
// encoding also has its own bounded copies/encoded representation). Four workers
// prepare sequential batches; neither a huge terminal nor font can grow a canvas.
const CROP_PIXELS: u32 = 262_144;

pub(crate) fn bounded_source(image: &DynamicImage) -> Arc<DynamicImage> {
    let pixels = if image.width() > SOURCE_WIDTH || image.height() > SOURCE_HEIGHT {
        image.thumbnail(SOURCE_WIDTH, SOURCE_HEIGHT).to_rgba8()
    } else {
        image.to_rgba8()
    };
    Arc::new(DynamicImage::ImageRgba8(pixels))
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CropKey {
    pub(crate) path: Arc<str>,
    revision: u64,
    canvas: Size,
    crop: Rect,
}

enum CropState {
    Queued,
    Preparing,
    Ready(Option<Protocol>),
}

#[derive(Default)]
pub struct StackPreviewCache {
    sources: HashMap<Arc<str>, (u64, Arc<DynamicImage>, (u32, u32))>,
    crops: HashMap<CropKey, CropState>,
    order: VecDeque<CropKey>,
    revision: u64,
    workers: usize,
}

pub struct CropResults(Vec<(CropKey, Option<Protocol>)>);

pub(crate) struct CropWork(Vec<(CropKey, Arc<DynamicImage>)>);

impl CropWork {
    // Called only by the existing media worker (or deterministic unit tests).
    pub(crate) fn prepare(self, picker: &mut Picker) -> CropResults {
        let font = picker.font_size();
        CropResults(
            self.0
                .into_iter()
                .map(|(key, source)| {
                    let width = u32::from(key.canvas.width) * u32::from(font.width);
                    let height = u32::from(key.canvas.height) * u32::from(font.height);
                    let protocol = if width != 0
                        && height != 0
                        && width <= 1024
                        && height <= 512
                        && width.saturating_mul(height) <= CROP_PIXELS
                    {
                        let fitted = Resize::Scale(None).resize(&source, font, key.canvas, None);
                        let crop = key.crop;
                        let pixels = fitted.crop_imm(
                            u32::from(crop.x) * u32::from(font.width),
                            u32::from(crop.y) * u32::from(font.height),
                            u32::from(crop.width) * u32::from(font.width),
                            u32::from(crop.height) * u32::from(font.height),
                        );
                        picker
                            .new_protocol(
                                pixels,
                                Size::new(crop.width, crop.height),
                                Resize::Fit(None),
                            )
                            .ok()
                    } else {
                        None
                    };
                    (key, protocol)
                })
                .collect(),
        )
    }
}

impl StackPreviewCache {
    #[cfg(test)]
    pub(crate) fn install(&mut self, path: Arc<str>, source: Arc<DynamicImage>) {
        let dimensions = (source.width(), source.height());
        self.install_original(path, source, dimensions);
    }

    pub(crate) fn install_original(
        &mut self,
        path: Arc<str>,
        source: Arc<DynamicImage>,
        dimensions: (u32, u32),
    ) {
        self.remove(&path);
        if source.width() == 0
            || source.height() == 0
            || source.width() > SOURCE_WIDTH
            || source.height() > SOURCE_HEIGHT
            || source.as_bytes().len() > (SOURCE_WIDTH * SOURCE_HEIGHT * 4) as usize
            || dimensions.0 == 0
            || dimensions.1 == 0
            || self.sources.len() >= SOURCE_CAPACITY
        {
            return;
        }
        self.revision = self.revision.wrapping_add(1);
        self.sources
            .insert(path, (self.revision, source, dimensions));
    }

    #[cfg(test)]
    pub(crate) fn contains_source(&self, path: &Arc<str>) -> bool {
        self.sources.contains_key(path)
    }

    pub(crate) fn was_downsampled(&self, path: &Arc<str>) -> bool {
        self.sources
            .get(path)
            .is_some_and(|(_, image, (width, height))| {
                *width > image.width() || *height > image.height()
            })
    }

    // Preserve original Fit geometry without retaining the original pixel buffer.
    pub(crate) fn fit_for(&self, path: &Arc<str>, font: FontSize, available: Size) -> Option<Size> {
        let (_, _, (width, height)) = self.sources.get(path)?;
        if font.width == 0 || font.height == 0 || available.width == 0 || available.height == 0 {
            return None;
        }
        let ratio = (f64::from(available.width) * f64::from(font.width) / f64::from(*width))
            .min(f64::from(available.height) * f64::from(font.height) / f64::from(*height))
            .min(1.0);
        let width = (f64::from(*width) * ratio).round().max(1.0) as u32;
        let height = (f64::from(*height) * ratio).round().max(1.0) as u32;
        Some(Size::new(
            width.div_ceil(u32::from(font.width)) as u16,
            height.div_ceil(u32::from(font.height)) as u16,
        ))
    }

    pub(crate) fn remove(&mut self, path: &Arc<str>) {
        self.sources.remove(path);
        self.crops.retain(|key, _| &key.path != path);
        self.order.retain(|key| &key.path != path);
    }

    pub(crate) fn key(&self, path: &Arc<str>, canvas: Size, crop: Rect) -> Option<CropKey> {
        let (revision, _, _) = self.sources.get(path)?;
        if crop.is_empty() || crop.right() > canvas.width || crop.bottom() > canvas.height {
            return None;
        }
        Some(CropKey {
            path: path.clone(),
            revision: *revision,
            canvas,
            crop,
        })
    }

    pub(crate) fn request(&mut self, key: CropKey) -> bool {
        if self
            .sources
            .get(&key.path)
            .is_none_or(|(revision, _, _)| *revision != key.revision)
        {
            return false;
        }
        if !self.crops.contains_key(&key) {
            if self.crops.len() >= CROP_CAPACITY
                && let Some(oldest) = self.order.pop_front()
            {
                self.crops.remove(&oldest);
            }
            self.crops.insert(key.clone(), CropState::Queued);
        }
        self.order.retain(|existing| existing != &key);
        self.order.push_back(key.clone());
        matches!(self.crops.get(&key), Some(CropState::Queued))
    }

    pub(crate) fn get(&self, key: &CropKey) -> Option<&Protocol> {
        match self.crops.get(key) {
            Some(CropState::Ready(Some(protocol))) => Some(protocol),
            _ => None,
        }
    }

    pub(crate) fn take_work(&mut self) -> Option<CropWork> {
        if self.workers >= WORKERS {
            return None;
        }
        let mut work = Vec::new();
        for key in &self.order {
            if matches!(self.crops.get(key), Some(CropState::Queued))
                && let Some((revision, source, _)) = self.sources.get(&key.path)
                && *revision == key.revision
            {
                work.push((key.clone(), source.clone()));
                self.crops.insert(key.clone(), CropState::Preparing);
                if work.len() == BATCH_SIZE {
                    break;
                }
            }
        }
        if work.is_empty() {
            return None;
        }
        self.workers += 1;
        Some(CropWork(work))
    }

    pub(crate) fn finish(&mut self, results: CropResults) {
        self.workers = self.workers.saturating_sub(1);
        for (key, protocol) in results.0 {
            if self
                .sources
                .get(&key.path)
                .is_some_and(|(revision, _, _)| *revision == key.revision)
                && matches!(self.crops.get(&key), Some(CropState::Preparing))
            {
                self.crops.insert(key, CropState::Ready(protocol));
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.crops.len()
    }
}
