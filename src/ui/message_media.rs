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
                    if !render_image || app.image_cache.get_mut(&data.path).is_none() {
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

    use crate::app::test_support::TestApp;

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
