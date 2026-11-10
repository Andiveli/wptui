//! Display-only adjuncts: source identity and fixed geometry, never image preparation.
use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    style::{Style, Stylize},
    text::Line,
    widgets::{Paragraph, Widget},
};
use ratatui_image::ResizeEncodeRender;
use whatsrust as wr;

use super::message_list::{MessageTextMode, inline_content_lines};
use crate::app::{
    App,
    events::{MediaRenderEffect, MediaRenderPlan},
    native_link_thumbnails::ThumbnailKey,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Summary<'a> {
    pub(crate) title: &'a std::sync::Arc<str>,
    pub(crate) description: &'a std::sync::Arc<str>,
    pub(crate) thumbnail: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OwnedSummary {
    title: std::sync::Arc<str>,
    description: std::sync::Arc<str>,
    thumbnail: bool,
}

impl From<Summary<'_>> for OwnedSummary {
    fn from(summary: Summary<'_>) -> Self {
        Self {
            title: summary.title.clone(),
            description: summary.description.clone(),
            thumbnail: summary.thumbnail,
        }
    }
}

impl OwnedSummary {
    pub(crate) fn matches(&self, summary: Summary<'_>) -> bool {
        &self.title == summary.title
            && &self.description == summary.description
            && self.thumbnail == summary.thumbnail
    }

    pub(crate) fn summary(&self) -> Summary<'_> {
        Summary {
            title: &self.title,
            description: &self.description,
            thumbnail: self.thumbnail,
        }
    }
}

pub(crate) fn summary<'a>(
    app: &'a App,
    message: &wr::Message,
    mode: MessageTextMode,
) -> Option<Summary<'a>> {
    if !mode.incoming_urls(message) {
        return None;
    }
    let wr::MessageContent::Text(source) = &message.message else {
        return None;
    };
    let stored = app.messages.get(&message.info.id)?;
    if stored.info.chat != message.info.chat
        || !matches!(&stored.message, wr::MessageContent::Text(text) if text == source)
    {
        return None;
    }
    let (range, preview) = app.native_link_preview(&message.info.id)?;
    if !super::markdown::Markdown::new(source, None).accepts_incoming_url(
        source,
        &range,
        &preview.matched_text,
    ) {
        return None;
    }
    Some(Summary {
        title: &preview.title,
        description: &preview.description,
        thumbnail: !preview.jpeg_thumbnail.is_empty(),
    })
}

impl Summary<'_> {
    pub(crate) fn image_size(self, width: usize) -> Option<Size> {
        (self.thumbnail && width > 1).then(|| Size::new(width.min(24) as u16, 6))
    }

    fn lines(self, width: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for (text, limit, style) in [
            (self.title, 2, Style::default().bold()),
            (self.description, 3, Style::default().dark_gray()),
        ] {
            if !text.is_empty() {
                lines.extend(
                    inline_content_lines(text, &[], None, width.max(1))
                        .into_iter()
                        .take(limit)
                        .map(|line| line.style(style)),
                );
            }
        }
        lines
    }

    pub(crate) fn height(self, width: usize) -> usize {
        self.lines(width).len()
            + self
                .image_size(width)
                .map_or(0, |size| usize::from(size.height))
    }
}

/// Translate an origin-zero temporary message buffer back to the actual frame.
/// The copy window, not the temporary buffer's area, is the visibility authority.
#[derive(Clone, Copy)]
pub(crate) struct PaintWindow {
    pub(crate) viewport: Rect,
    pub(crate) frame: Rect,
    pub(crate) offset_x: i64,
    pub(crate) offset_y: i64,
}

impl PaintWindow {
    pub(crate) fn direct(viewport: Rect, frame: Rect) -> Self {
        Self {
            viewport,
            frame,
            offset_x: 0,
            offset_y: 0,
        }
    }

    fn safe(self, area: Rect, app: &App) -> bool {
        let (Ok(x), Ok(y)) = (
            u16::try_from(i64::from(area.x) + self.offset_x),
            u16::try_from(i64::from(area.y) + self.offset_y),
        ) else {
            return false;
        };
        let screen = Rect::new(x, y, area.width, area.height);
        !screen.is_empty()
            && screen.intersection(self.viewport) == screen
            && screen.intersection(self.frame) == screen
            && !super::native_image_occluded(app, self.frame, screen)
    }
}

pub(crate) fn render(
    buf: &mut Buffer,
    app: &mut App,
    message: &wr::Message,
    mode: MessageTextMode,
    area: Rect,
    window: PaintWindow,
    render_image: bool,
    plan: &mut MediaRenderPlan,
) {
    let Some(summary) = summary(app, message, mode) else {
        return;
    };
    let lines = summary.lines(area.width as usize);
    let image_size = summary.image_size(area.width as usize);
    let text_height = lines.len() as u16;
    Paragraph::new(lines).render(
        Rect::new(area.x, area.y, area.width, area.height.min(text_height)),
        buf,
    );
    let Some(size) = image_size else {
        return;
    };
    let image = Rect::new(
        area.x,
        area.y.saturating_add(text_height),
        size.width,
        size.height,
    );
    // Gate BEFORE effect collection and protocol rendering. Rendering then clearing
    // would still transmit/mark Kitty state for cells the user cannot see.
    if !render_image
        || image.intersection(area) != image
        || image.intersection(buf.area) != image
        || !window.safe(image, app)
    {
        return;
    }
    let wr::MessageContent::Text(source) = &message.message else {
        return;
    };
    let Some((_, metadata)) = app.native_link_preview(&message.info.id) else {
        return;
    };
    let key = ThumbnailKey {
        chat: message.info.chat.clone(),
        message_id: message.info.id.clone(),
        source: source.clone(),
        matched_url: metadata.matched_text.clone(),
        size,
    };
    // Refresh demand in every cache state; admission and all byte work are post-draw.
    plan.append(MediaRenderEffect::LoadNativeLinkThumbnail(key.clone()));
    if let Some(protocol) = app.native_link_thumbnails.protocol_mut(&key) {
        // This calls only the prepared-state paint API, not StatefulImage's
        // resize_encode_render / needs_resize path.
        ResizeEncodeRender::render(protocol, image, buf);
    }
}
