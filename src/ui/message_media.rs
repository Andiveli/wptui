use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    text::Line,
    widgets::{Paragraph, StatefulWidget, Widget},
};
use ratatui_image::StatefulImage;
use whatsrust::{self as wr, FileKind};

use crate::app::events::MediaRenderEffect;
use crate::app::{App, FileMeta, Metadata};

use super::MessageTextMode;
use super::message_formatting::MediaStatus;
use super::message_helpers::{StatusLabel, inline_content_lines};

pub fn preview_height(kind: &FileKind) -> usize {
    match kind {
        FileKind::Video => super::message_layout::VIDEO_HEIGHT,
        _ => super::message_layout::IMAGE_HEIGHT,
    }
}

fn content_height(file: &wr::FileContent) -> usize {
    match file.kind {
        FileKind::Image | FileKind::Sticker | FileKind::Video => preview_height(&file.kind),
        FileKind::Audio => 2,
        FileKind::Document => 1,
    }
}

fn should_request_download(message_id: &wr::MessageId) -> bool {
    !App::is_pending_message_id(message_id)
}

fn caption_lines(
    caption: Option<&str>,
    mention_ranges: &[std::ops::Range<usize>],
    status: Option<StatusLabel>,
    width: usize,
    text_mode: MessageTextMode,
) -> Vec<Line<'static>> {
    let caption = caption.unwrap_or_default();
    match text_mode {
        MessageTextMode::Chat => inline_content_lines(caption, mention_ranges, status, width),
        MessageTextMode::Status => inline_content_lines(caption, mention_ranges, status, width),
    }
}

pub fn render_file(
    buf: &mut Buffer,
    message_id: &wr::MessageId,
    data: &wr::FileContent,
    status: Option<StatusLabel>,
    app: &mut App,
    media_render_plan: &mut crate::app::events::MediaRenderPlan,
    content_area: Rect,
    render_image: bool,
    alignment: Alignment,
    text_mode: MessageTextMode,
) {
    let is_audio = matches!(data.kind, FileKind::Audio);
    let audio_duration = is_audio
        .then(|| app.audio_durations.get(data.path.as_ref()).copied())
        .flatten();
    let [media_area, caption_area] = Layout::vertical([
        Constraint::Length(content_height(data) as u16),
        Constraint::Min(0),
    ])
    .areas(content_area);

    match app.metadata.get(message_id) {
        None => {
            super::media_paragraph(
                data.path.as_ref(),
                MediaStatus::Pending,
                is_audio,
                data.path.as_ref(),
                audio_duration,
                alignment,
            )
            .render(media_area, buf);
            if should_request_download(message_id) {
                media_render_plan.append(MediaRenderEffect::DownloadFile(
                    message_id.clone(),
                    data.file_id.clone(),
                ));
            }
        }
        Some(Metadata::File(meta)) => match meta {
            FileMeta::Downloaded => {
                super::media_paragraph(
                    data.path.as_ref(),
                    MediaStatus::Downloaded,
                    is_audio,
                    data.path.as_ref(),
                    audio_duration,
                    alignment,
                )
                .render(media_area, buf);
                if matches!(
                    data.kind,
                    FileKind::Image | FileKind::Sticker | FileKind::Video
                ) && !matches!(
                    app.metadata.get(message_id),
                    Some(Metadata::File(FileMeta::Loading))
                ) {
                    media_render_plan
                        .append(MediaRenderEffect::LoadFilePreview(message_id.clone()));
                }
            }
            FileMeta::Downloading => super::media_paragraph(
                data.path.as_ref(),
                MediaStatus::Downloading,
                is_audio,
                data.path.as_ref(),
                audio_duration,
                alignment,
            )
            .render(media_area, buf),
            FileMeta::DownloadFailed => super::media_paragraph(
                data.path.as_ref(),
                MediaStatus::DownloadFailed,
                is_audio,
                data.path.as_ref(),
                audio_duration,
                alignment,
            )
            .render(media_area, buf),
            FileMeta::LoadFailed => super::media_paragraph(
                data.path.as_ref(),
                MediaStatus::LoadFailed,
                is_audio,
                data.path.as_ref(),
                audio_duration,
                alignment,
            )
            .render(media_area, buf),
            FileMeta::Loading => {
                log::trace!("Rendering loading for {}", message_id);
                super::media_paragraph(
                    data.path.as_ref(),
                    MediaStatus::Loading,
                    is_audio,
                    data.path.as_ref(),
                    audio_duration,
                    alignment,
                )
                .render(media_area, buf);
            }
            FileMeta::Loaded => match data.kind {
                FileKind::Image | FileKind::Sticker | FileKind::Video => {
                    let placeholder = if matches!(data.kind, FileKind::Video) {
                        "🎬"
                    } else {
                        "🖼"
                    };
                    if render_image
                        && matches!(data.kind, FileKind::Sticker)
                        && app.sticker_playback.contains_key(&data.path)
                    {
                        app.visible_stickers.insert(data.path.clone());
                        app.touch_sticker_cache(&data.path);
                        if let Some(playback) = app.sticker_playback.get_mut(&data.path) {
                            StatefulImage::default().render(
                                media_area,
                                buf,
                                &mut playback.frames[playback.current],
                            );
                        }
                    } else if !render_image || app.image_cache.get_mut(&data.path).is_none() {
                        Paragraph::new(placeholder)
                            .alignment(alignment)
                            .render(media_area, buf);
                    } else {
                        app.touch_image_cache(&data.path);
                        if let Some(image) = app.image_cache.get_mut(&data.path) {
                            StatefulImage::default().render(media_area, buf, image);
                        } else {
                            Paragraph::new(placeholder)
                                .alignment(alignment)
                                .render(media_area, buf);
                        }
                    }
                }
                FileKind::Audio | FileKind::Document => super::media_paragraph(
                    data.path.as_ref(),
                    MediaStatus::Downloaded,
                    is_audio,
                    data.path.as_ref(),
                    audio_duration,
                    alignment,
                )
                .render(media_area, buf),
            },
        },
    }

    // Only the media/path region is actionable. Caption links have their own
    // URL hits; neither the empty message background nor a sticker is a target.
    if !matches!(status, Some(super::message_helpers::StatusLabel::Deleted))
        && !App::is_pending_message_id(message_id)
        && matches!(
            data.kind,
            FileKind::Image | FileKind::Video | FileKind::Audio | FileKind::Document
        )
    {
        let painted_image = matches!(data.kind, FileKind::Image | FileKind::Video)
            && render_image
            && matches!(
                app.metadata.get(message_id),
                Some(Metadata::File(FileMeta::Loaded))
            )
            && app.image_cache.contains_key(&data.path);
        for row in media_area.rows() {
            if painted_image {
                app.mouse_hit_map
                    .media
                    .push(crate::app::mouse_input::MediaHit {
                        area: row,
                        id: message_id.clone(),
                        kind: data.kind.clone(),
                    });
                continue;
            }
            for position in row.columns() {
                let symbol = buf[position].symbol();
                if symbol.trim().is_empty() {
                    continue;
                }
                let width = Line::from(symbol)
                    .width()
                    .max(1)
                    .min(usize::from(row.right() - position.x)) as u16;
                app.mouse_hit_map
                    .media
                    .push(crate::app::mouse_input::MediaHit {
                        area: Rect::new(position.x, position.y, width, 1),
                        id: message_id.clone(),
                        kind: data.kind.clone(),
                    });
            }
        }
    }

    if data.caption.is_some() || status.is_some() {
        let mention_ranges = data
            .caption
            .as_deref()
            .map(|caption| wr::message_mention_ranges(message_id, caption))
            .unwrap_or_default();
        Paragraph::new(caption_lines(
            data.caption.as_deref(),
            &mention_ranges,
            status,
            content_area.width as usize,
            text_mode,
        ))
        .render(caption_area, buf);
        if let Some(caption) = data.caption.as_deref() {
            super::record_url_hits(
                app,
                message_id,
                caption,
                status,
                caption_area,
                Alignment::Left,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MessageTextMode, caption_lines, preview_height, should_request_download};
    use ratatui::{
        Terminal,
        backend::TestBackend,
        buffer::Buffer,
        layout::{Alignment, Rect},
        widgets::{Paragraph, Widget},
    };
    use whatsrust::{FileContent, FileKind};

    use crate::app::sticker_animation::{StickerPlayback, decode_animated_webp};
    use crate::app::test_support::TestApp;
    use crate::app::{FileMeta, Metadata};
    use ratatui::layout::Size;
    use ratatui_image::picker::{Picker, ProtocolType};
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };

    #[test]
    fn caption_cells_use_visual_lines_after_logical_wrapping() {
        let mut terminal = Terminal::new(TestBackend::new(4, 3)).unwrap();
        terminal
            .draw(|frame| {
                Paragraph::new(caption_lines(
                    Some("abc אבג 123"),
                    &[],
                    None,
                    4,
                    MessageTextMode::Chat,
                ))
                .render(frame.area(), frame.buffer_mut());
            })
            .unwrap();
        terminal
            .backend()
            .assert_buffer_lines(["abc ", "גבא ", "123 "]);
    }

    #[test]
    fn pending_local_media_does_not_request_download() {
        assert!(!should_request_download(&"local-send-42".into()));
        assert!(should_request_download(&"server-message".into()));
    }

    #[test]
    fn preview_height_matches_layout_contract() {
        assert_eq!(
            preview_height(&FileKind::Video),
            super::super::message_layout::VIDEO_HEIGHT
        );
        assert_eq!(
            preview_height(&FileKind::Image),
            super::super::message_layout::IMAGE_HEIGHT
        );
    }

    #[test]
    fn two_visible_sticker_previews_advance_together() {
        let mut app = TestApp::new();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 28));
        let mut plan = crate::app::events::MediaRenderPlan::default();
        let mut paths = Vec::new();
        for (index, y) in [(0, 0), (1, 14)] {
            let id = format!("sticker-{index}");
            let path = Arc::<str>::from(format!("stickers/{id}.webp"));
            let animation =
                decode_animated_webp(include_bytes!("../../tests/fixtures/sticker-animated.webp"))
                    .unwrap()
                    .unwrap();
            app.sticker_playback.insert(
                path.clone(),
                StickerPlayback::prepare(animation, &mut Picker::halfblocks(), Size::new(20, 14)),
            );
            app.metadata
                .insert(id.clone().into(), Metadata::File(FileMeta::Loaded));
            let file = FileContent {
                kind: FileKind::Sticker,
                path: path.clone(),
                ..Default::default()
            };
            super::render_file(
                &mut buffer,
                &id.into(),
                &file,
                None,
                &mut app,
                &mut plan,
                Rect::new(0, y, 20, 14),
                true,
                Alignment::Left,
                MessageTextMode::Chat,
            );
            paths.push(path);
        }
        assert!(plan.into_effects().is_empty());
        assert_eq!(app.visible_stickers.len(), 2);
        assert!(paths.iter().all(|path| app.visible_stickers.contains(path)));
        let start = Instant::now();
        assert!(!app.advance_visible_stickers(start));
        assert!(app.advance_visible_stickers(start + Duration::from_millis(80)));
        assert!(
            paths
                .iter()
                .all(|path| app.sticker_playback[path].current == 1)
        );
        assert_ne!(buffer[(0, 0)].symbol(), " ");
        assert_ne!(buffer[(0, 14)].symbol(), " ");
    }

    #[test]
    fn two_visible_kitty_stickers_emit_distinct_frames() {
        let mut app = TestApp::new();
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(ProtocolType::Kitty);
        let mut paths = Vec::new();
        for index in 0..2 {
            let id = format!("kitty-sticker-{index}");
            let path = Arc::<str>::from(format!("stickers/{id}.webp"));
            let animation =
                decode_animated_webp(include_bytes!("../../tests/fixtures/sticker-animated.webp"))
                    .unwrap()
                    .unwrap();
            app.sticker_playback.insert(
                path.clone(),
                StickerPlayback::prepare(animation, &mut picker, Size::new(20, 14)),
            );
            app.metadata
                .insert(id.into(), Metadata::File(FileMeta::Loaded));
            paths.push(path);
        }

        let paint = |app: &mut TestApp| {
            let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 28));
            let mut plan = crate::app::events::MediaRenderPlan::default();
            for (index, path) in paths.iter().enumerate() {
                let file = FileContent {
                    kind: FileKind::Sticker,
                    path: path.clone(),
                    ..Default::default()
                };
                super::render_file(
                    &mut buffer,
                    &format!("kitty-sticker-{index}").into(),
                    &file,
                    None,
                    app,
                    &mut plan,
                    Rect::new(0, index as u16 * 14, 20, 14),
                    true,
                    Alignment::Left,
                    MessageTextMode::Chat,
                );
            }
            assert!(plan.into_effects().is_empty());
            [
                buffer[(0, 0)].symbol().to_owned(),
                buffer[(0, 14)].symbol().to_owned(),
            ]
        };

        let first = paint(&mut app);
        assert_eq!(app.visible_stickers.len(), 2);
        let image_id = |symbol: &str| {
            let header = symbol
                .split("\x1b_Gq=2,i=")
                .nth(1)
                .expect("Kitty transmission");
            header.split(',').next().unwrap().to_owned()
        };
        let first_ids = first.each_ref().map(|symbol| image_id(symbol));
        assert_ne!(first_ids[0], first_ids[1], "distinct virtual image IDs");
        for symbol in &first {
            assert!(
                symbol.contains(",a=T,U=1,"),
                "virtual placement transmission"
            );
            assert!(symbol.contains("\u{10EEEE}"), "Kitty image placeholder");
        }

        let start = Instant::now();
        assert!(!app.advance_visible_stickers(start));
        assert!(app.advance_visible_stickers(start + Duration::from_millis(80)));
        assert!(
            paths
                .iter()
                .all(|path| app.sticker_playback[path].current == 1)
        );
        let second = paint(&mut app);
        let second_ids = second.each_ref().map(|symbol| image_id(symbol));
        assert_ne!(second_ids[0], second_ids[1]);
        for index in 0..2 {
            assert_ne!(first_ids[index], second_ids[index], "new frame image ID");
            assert_ne!(
                first[index], second[index],
                "changed Kitty frame placeholder"
            );
            assert!(second[index].contains(",a=T,U=1,"));
        }
    }

    #[test]
    fn rendering_media_does_not_dispatch_runtime_effects() {
        let mut app = TestApp::new();
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 8));
        let mut media_plan = crate::app::events::MediaRenderPlan::default();
        let file = FileContent {
            kind: FileKind::Image,
            path: "image.png".into(),
            ..Default::default()
        };

        super::render_file(
            &mut buf,
            &"server-message".into(),
            &file,
            None,
            &mut app,
            &mut media_plan,
            Rect::new(0, 0, 20, 8),
            true,
            Alignment::Left,
            MessageTextMode::Chat,
        );

        assert!(app.rx.try_recv().is_err());
        assert!(matches!(
            media_plan.into_effects().as_slice(),
            [crate::app::events::MediaRenderEffect::DownloadFile(message_id, _)]
                if message_id == &"server-message".into()
        ));
    }
}
