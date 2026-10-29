use std::{io::Cursor, time::Duration};

use image::{AnimationDecoder, Frame, ImageDecoder, RgbaImage, codecs::webp::WebPDecoder};

const MAX_DIMENSION: u32 = 512;
const MAX_SOURCE_FRAMES: usize = 256;
const MAX_RETAINED_FRAMES: usize = 32;
const MAX_DECODE_WORK_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RETAINED_BYTES: u64 = 16 * 1024 * 1024;

/// A composited canvas and the time it remains visible.
pub struct StickerFrame {
    pub image: RgbaImage,
    pub duration: Duration,
}

pub struct StickerAnimation {
    pub frames: Vec<StickerFrame>,
}

/// Returns `None` for static WebP so the caller can retain its existing still-image path.
/// An animation exceeding any budget becomes its first composited frame only.
pub fn decode_animated_webp(bytes: &[u8]) -> image::ImageResult<Option<StickerAnimation>> {
    let decoder = WebPDecoder::new(Cursor::new(bytes))?;
    if !decoder.has_animation() {
        return Ok(None);
    }
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(image::ImageError::Limits(
            image::error::LimitError::from_kind(image::error::LimitErrorKind::DimensionError),
        ));
    }
    collect_frames(decoder.into_frames()).map(Some)
}

fn collect_frames(
    frames: impl Iterator<Item = image::ImageResult<Frame>>,
) -> image::ImageResult<StickerAnimation> {
    let mut retained = Vec::new();
    let mut work = 0u64;
    let mut retained_bytes = 0u64;
    let mut truncated = false;
    for (index, result) in frames.take(MAX_SOURCE_FRAMES + 1).enumerate() {
        if index == MAX_SOURCE_FRAMES {
            truncated = true;
            break;
        }
        let frame = result?;
        let image = frame.buffer();
        let bytes = image.as_raw().len() as u64;
        if image.width() > MAX_DIMENSION
            || image.height() > MAX_DIMENSION
            || work.saturating_add(bytes) > MAX_DECODE_WORK_BYTES
            || retained.len() == MAX_RETAINED_FRAMES
            || retained_bytes.saturating_add(bytes) > MAX_RETAINED_BYTES
        {
            truncated = true;
            break;
        }
        work += bytes;
        retained_bytes += bytes;
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let millis = if denominator == 0 {
            100
        } else {
            u64::from(numerator) / u64::from(denominator)
        };
        retained.push(StickerFrame {
            image: frame.into_buffer(),
            duration: Duration::from_millis(millis.clamp(20, 10_000)),
        });
    }
    if retained.is_empty() {
        return Err(image::ImageError::Limits(
            image::error::LimitError::from_kind(image::error::LimitErrorKind::InsufficientMemory),
        ));
    }
    // An incomplete prefix must never masquerade as a complete looping animation.
    if truncated {
        retained.truncate(1);
    }
    Ok(StickerAnimation { frames: retained })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Delay, Frame};

    fn frame(value: u8, millis: u32) -> image::ImageResult<Frame> {
        Ok(Frame::from_parts(
            RgbaImage::from_pixel(2, 2, image::Rgba([value, 0, 0, 255])),
            0,
            0,
            Delay::from_numer_denom_ms(millis, 1),
        ))
    }

    #[test]
    fn decodes_animated_webp_sticker() {
        let bytes = include_bytes!("../../tests/fixtures/sticker-animated.webp");
        let animation = decode_animated_webp(bytes).unwrap().unwrap();
        assert_eq!(animation.frames.len(), 2);
        assert_eq!(
            animation.frames[0].image.get_pixel(0, 0).0[..3],
            [255, 0, 0]
        );
        assert_eq!(
            animation.frames[1].image.get_pixel(0, 0).0[..3],
            [0, 0, 254]
        );
        assert_eq!(animation.frames[0].duration, Duration::from_millis(80));
        assert_eq!(animation.frames[1].duration, Duration::from_millis(120));
    }

    #[test]
    fn retains_composited_frames_and_timing() {
        let animation = collect_frames(vec![frame(1, 80), frame(2, 120)].into_iter()).unwrap();
        assert_eq!(animation.frames.len(), 2);
        assert_eq!(animation.frames[0].image.get_pixel(0, 0).0[0], 1);
        assert_eq!(animation.frames[1].image.get_pixel(0, 0).0[0], 2);
        assert_eq!(animation.frames[1].duration, Duration::from_millis(120));
    }

    #[test]
    fn decoded_work_and_retention_limits_fall_back() {
        let oversized = || {
            Ok(Frame::from_parts(
                RgbaImage::new(MAX_DIMENSION, MAX_DIMENSION),
                0,
                0,
                Delay::from_numer_denom_ms(80, 1),
            ))
        };
        let animation = collect_frames(std::iter::repeat_with(oversized).take(70)).unwrap();
        assert_eq!(animation.frames.len(), 1);
        let animation = collect_frames(vec![frame(1, 80), frame(2, 80)].into_iter()).unwrap();
        assert_eq!(animation.frames.len(), 2);
    }

    #[test]
    fn overflow_returns_only_first_frame() {
        let frames = (0..MAX_SOURCE_FRAMES + 1).map(|n| frame(n as u8, 40));
        let animation = collect_frames(frames).unwrap();
        assert_eq!(animation.frames.len(), 1);
    }
}
