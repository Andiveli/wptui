use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::media::MediaRoot;
use log::warn;

#[cfg(test)]
mod tests;

/// WhatsApp embeds a tiny thumbnail (a few hundred bytes, ~72px) with video
/// messages; anything at least this large is a real extracted frame.
const VIDEO_THUMBNAIL_MIN_BYTES: u64 = 4096;

/// True when an already-confined sidecar holds a usable frame rather than
/// WhatsApp's narrow embedded thumbnail.
pub(crate) fn has_decent_video_thumbnail(file: &fs::File) -> bool {
    file.metadata()
        .is_ok_and(|meta| meta.len() >= VIDEO_THUMBNAIL_MIN_BYTES)
}

/// Decode a real video frame in memory without writing a sidecar. On failure
/// the caller may still decode an existing contained WhatsApp thumbnail.
pub(crate) fn generate_video_thumbnail(
    root: &MediaRoot,
    video_relative: &Path,
) -> Option<image::DynamicImage> {
    let jpeg = generate_video_thumbnail_with_ffmpeg(root, video_relative, Path::new("ffmpeg"))?;
    image::load_from_memory(&jpeg).ok()
}

fn generate_video_thumbnail_with_ffmpeg(
    root: &MediaRoot,
    video_relative: &Path,
    executable: &Path,
) -> Option<Vec<u8>> {
    const MAX_JPEG_BYTES: u64 = 8 * 1024 * 1024;

    // ffmpeg sees only a regular file opened beneath the pinned root. Its
    // generated image stays in memory, so renames or hardlinks cannot turn a
    // sidecar write into a write to another file.
    let mut video = root.open_media_file(video_relative).ok()?;
    let attempts: [&[&str]; 2] = [
        &["-loglevel", "error", "-ss", "1"],
        &["-loglevel", "error"], // Very short videos have no frame at 1s.
    ];
    for args in attempts {
        video.seek(SeekFrom::Start(0)).ok()?;
        let input = video.try_clone().ok()?;
        // The seekable fd: input permits only the inherited regular stdin
        // descriptor. Restrict nested demuxer protocols so crafted media cannot
        // make ffmpeg open a second local path or network URL.
        let mut child = match Command::new(executable)
            .args(args)
            .args(["-protocol_whitelist", "fd", "-i", "fd:"])
            .args(["-frames:v", "1", "-q:v", "4", "-f", "image2pipe"])
            .args(["-vcodec", "mjpeg", "pipe:1"])
            .stdin(Stdio::from(input))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                warn!("ffmpeg thumbnail extraction failed: {err}");
                return None;
            }
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        };
        let mut jpeg = Vec::new();
        let read = stdout.take(MAX_JPEG_BYTES + 1).read_to_end(&mut jpeg);
        if read.is_err() || jpeg.len() as u64 > MAX_JPEG_BYTES {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        match child.wait() {
            Ok(status) if status.success() && jpeg.len() as u64 >= VIDEO_THUMBNAIL_MIN_BYTES => {
                return Some(jpeg);
            }
            Ok(_) => continue,
            Err(err) => {
                warn!("ffmpeg thumbnail extraction failed: {err}");
                return None;
            }
        }
    }
    None
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
