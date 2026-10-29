use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use ratatui::layout::Size;
use ratatui_image::{Resize, ResizeEncodeRender, picker::Picker, protocol::StatefulProtocol};

use super::sticker_decode::StickerAnimation;

const MAX_ANIMATION_JOBS: usize = 2;

pub struct AnimationSlots(AtomicUsize);

impl AnimationSlots {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(AtomicUsize::new(0)))
    }

    pub fn try_acquire(self: &Arc<Self>) -> Option<AnimationSlot> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_ANIMATION_JOBS).then_some(n + 1)
            })
            .ok()?;
        Some(AnimationSlot(Arc::clone(self)))
    }
}

pub struct AnimationSlot(Arc<AnimationSlots>);

impl Drop for AnimationSlot {
    fn drop(&mut self) {
        self.0.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Protocols are prepared on the media worker, never inside a draw.
pub struct StickerPlayback {
    pub frames: Vec<StatefulProtocol>,
    pub durations: Vec<Duration>,
    pub current: usize,
    pub deadline: Option<Instant>,
    pub(crate) slot: Option<AnimationSlot>,
}

impl StickerPlayback {
    pub fn prepare(animation: StickerAnimation, picker: &mut Picker, size: Size) -> Self {
        let mut frames = Vec::with_capacity(animation.frames.len());
        let mut durations = Vec::with_capacity(animation.frames.len());
        for frame in animation.frames {
            let mut protocol =
                picker.new_resize_protocol(image::DynamicImage::ImageRgba8(frame.image));
            protocol.resize_encode(&Resize::Scale(None), size);
            frames.push(protocol);
            durations.push(frame.duration);
        }
        Self {
            frames,
            durations,
            current: 0,
            deadline: None,
            slot: None,
        }
    }

    pub fn advance(&mut self, now: Instant) -> bool {
        if self.frames.len() < 2 {
            return false;
        }
        match self.deadline {
            None => {
                self.deadline = Some(now + self.durations[self.current]);
                false
            }
            Some(deadline) if now < deadline => false,
            Some(_) => {
                self.current = (self.current + 1) % self.frames.len();
                self.deadline = Some(now + self.durations[self.current]);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slots_remain_bounded_until_result_is_consumed_or_dropped() {
        let slots = AnimationSlots::new();
        let first = slots.try_acquire().unwrap();
        let second = slots.try_acquire().unwrap();
        assert!(slots.try_acquire().is_none());
        drop(first);
        assert!(slots.try_acquire().is_some());
        drop(second);
    }

    #[test]
    fn playback_advances_only_at_deadline_and_resets_after_pause() {
        let animation = super::super::sticker_decode::decode_animated_webp(include_bytes!(
            "../../tests/fixtures/sticker-animated.webp"
        ))
        .unwrap()
        .unwrap();
        let mut playback =
            StickerPlayback::prepare(animation, &mut Picker::halfblocks(), Size::new(2, 2));
        let start = Instant::now();
        assert!(!playback.advance(start));
        assert!(!playback.advance(start + Duration::from_millis(79)));
        assert!(playback.advance(start + Duration::from_millis(80)));
        assert_eq!(playback.current, 1);
        playback.deadline = None;
        assert!(!playback.advance(start + Duration::from_secs(10)));
        assert_eq!(playback.current, 1);
    }
}
