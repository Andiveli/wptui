use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::media::MediaRoot;
use log::warn;

#[cfg(test)]
mod tests;

/// WhatsApp embeds a tiny thumbnail (a few hundred bytes, ~72px) with video
/// messages; anything at least this large is a real extracted frame.
const VIDEO_THUMBNAIL_MIN_BYTES: u64 = 4096;

/// True when the sidecar already contains a usable video frame (not the tiny
/// embedded WhatsApp thumbnail that renders as a narrow sliver).
pub(crate) fn has_decent_video_thumbnail(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.len() >= VIDEO_THUMBNAIL_MIN_BYTES)
        .unwrap_or(false)
}

/// Extracts a real frame from a video file with ffmpeg into the `.jpg`
/// sidecar. Used so inline video previews show the actual frame instead of
/// WhatsApp's tiny embedded thumbnail. Best effort: on any failure the caller
/// falls back to the existing placeholder rendering.
pub(crate) fn generate_video_thumbnail(video_path: &Path, sidecar_path: &Path) {
    let attempts: [&[&str]; 2] = [
        // Seek 1s in so we skip the common black first frame.
        &["-y", "-loglevel", "error", "-ss", "1"],
        // Fallback for very short videos that start past 1s.
        &["-y", "-loglevel", "error"],
    ];
    for args in attempts {
        let status = Command::new("ffmpeg")
            .args(args)
            .arg("-i")
            .arg(video_path)
            .args(["-frames:v", "1", "-q:v", "4"])
            .arg(sidecar_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match status {
            Ok(status) if status.success() && has_decent_video_thumbnail(sidecar_path) => return,
            Ok(_) => continue,
            Err(err) => {
                warn!("ffmpeg thumbnail extraction failed: {err}");
                return;
            }
        }
    }
    warn!(
        "ffmpeg could not extract a thumbnail for {}",
        video_path.display()
    );
}

/// Reads the duration (in whole seconds) of an audio file with lofty.
/// Best effort: returns `None` for unreadable files, unsupported formats, or
/// files whose properties could not be resolved (lofty reports `Duration::ZERO`).
///
/// `guess_file_type()` sniffs the content (first 36 bytes) because the
/// extension map in lofty lacks `.oga` — the extension WhatsApp uses for
/// Opus voice notes — so extension-only probing would reject them.
pub(crate) fn probe_audio_duration(path: &Path) -> Option<u64> {
    use lofty::file::AudioFile;
    let duration = lofty::probe::Probe::open(path)
        .ok()?
        .guess_file_type()
        .ok()?
        .read()
        .ok()?
        .properties()
        .duration();
    (!duration.is_zero()).then_some(duration.as_secs())
}

/// Overlays a play-button marker (translucent circle + white triangle) on a
/// video thumbnail, matching Discord's in-app video preview styling.
pub(crate) fn apply_video_play_marker(image: &mut image::DynamicImage) {
    let mut rgba = image.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let min_dimension = width.min(height);
    if min_dimension < 24 {
        return;
    }

    let cx = width as f32 / 2.0 - 0.5;
    let cy = height as f32 / 2.0 - 0.5;
    let radius = (min_dimension as f32 * 0.14).clamp(10.0, 56.0);
    let radius_sq = radius * radius;

    // Translucent black circle
    for (x, y, pixel) in rgba.enumerate_pixels_mut() {
        let dx = x as f32 - cx;
        let dy = y as f32 - cy;
        if dx * dx + dy * dy <= radius_sq {
            blend_pixel(pixel, image::Rgba([0, 0, 0, 135]));
        }
    }

    // White play triangle
    let left = cx - radius * 0.24;
    let right = cx + radius * 0.42;
    let top = cy - radius * 0.42;
    let bottom = cy + radius * 0.42;
    let tri_min_x = left.floor().max(0.0) as u32;
    let tri_max_x = right.ceil().min(width.saturating_sub(1) as f32) as u32;
    let tri_min_y = top.floor().max(0.0) as u32;
    let tri_max_y = bottom.ceil().min(height.saturating_sub(1) as f32) as u32;

    for y in tri_min_y..=tri_max_y {
        let vertical = if y as f32 <= cy {
            ((y as f32 - top) / (cy - top)).clamp(0.0, 1.0)
        } else {
            ((bottom - y as f32) / (bottom - cy)).clamp(0.0, 1.0)
        };
        let row_left = left;
        let row_right = left + (right - left) * vertical;
        for x in tri_min_x..=tri_max_x {
            let xf = x as f32;
            if xf >= row_left && xf <= row_right {
                blend_pixel(rgba.get_pixel_mut(x, y), image::Rgba([245, 247, 250, 230]));
            }
        }
    }

    *image = image::DynamicImage::ImageRgba8(rgba);
}

fn blend_pixel(pixel: &mut image::Rgba<u8>, overlay: image::Rgba<u8>) {
    let alpha = u16::from(overlay.0[3]);
    let inverse_alpha = 255u16.saturating_sub(alpha);
    for channel in 0..3 {
        pixel.0[channel] = ((u16::from(overlay.0[channel]) * alpha
            + u16::from(pixel.0[channel]) * inverse_alpha
            + 127)
            / 255) as u8;
    }
    pixel.0[3] = pixel.0[3].max(overlay.0[3]);
}

/// Removes media files for purged status broadcasts, including the video
/// thumbnail sidecar (`videos/<id>.jpg`) that ffmpeg generates next to the
/// video. Missing files are ignored.
pub fn remove_owned_media_files(media_path: &Path, relative_paths: &[PathBuf]) {
    let Ok(root) = MediaRoot::new(media_path) else {
        return;
    };
    for rel in relative_paths {
        for candidate in [rel.clone(), rel.with_extension("jpg")] {
            if let Ok(path) = root.media_file(&candidate) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

pub fn remove_status_media_files(media_path: &Path, relative_paths: &[PathBuf]) {
    remove_owned_media_files(media_path, relative_paths);
}
