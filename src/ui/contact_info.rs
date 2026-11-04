use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Borders, Paragraph, StatefulWidget},
};
use ratatui_image::StatefulImage;

use super::truncate_with_ellipsis;
use crate::app::{App, contact_avatars::AvatarTarget, contact_info::SharedGroups};

pub(crate) fn render(frame: &mut Frame, app: &mut App<'_>, area: Rect) {
    let Some(info) = &app.contact_info else {
        return;
    };
    if area.is_empty() {
        return;
    }
    let contact = info.contact.clone();
    let scroll = info.scroll;
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new("Contact information").style(Style::default().fg(Color::Cyan).bold()),
        Rect::new(inner.x, inner.y, inner.width, 1.min(inner.height)),
    );
    if inner.height < 2 {
        return;
    }
    // A square portrait occupies 38 columns by 19 terminal rows on the wide
    // artboard. On short terminals reduce it before clipping identity/actions.
    let photo_height = 19.min(inner.height.saturating_sub(11));
    let photo_width = 38.min(inner.width.saturating_sub(2)).min(photo_height * 2);
    let photo = Rect::new(
        inner.x + inner.width.saturating_sub(photo_width) / 2,
        inner.y + 2,
        photo_width,
        photo_height,
    );
    let target = AvatarTarget::Contact(contact.clone());
    if !photo.is_empty() {
        if let Some(protocol) = app.contact_avatars.protocol_mut(&target) {
            StatefulImage::default().render(photo, frame.buffer_mut(), protocol);
        } else {
            frame.render_widget(
                Paragraph::new(app.contact_avatars.status_label(&target))
                    .style(Style::default().fg(Color::DarkGray)),
                photo,
            );
        }
    }
    let name = app.contact_name(&contact);
    let name = if name.is_empty() {
        "Unknown contact"
    } else {
        &name
    };
    let phone = app
        .contact_info_phone(&contact)
        .unwrap_or_else(|| "Unknown".into());
    let width = inner.width.saturating_sub(2) as usize;
    let mut lines = vec![
        Line::from(truncate_with_ellipsis(name, width)).bold(),
        Line::from("Phone").fg(Color::DarkGray),
        Line::from(truncate_with_ellipsis(&phone, width)),
        Line::from(""),
    ];
    let info = app.contact_info.as_ref().unwrap();
    match &info.groups {
        SharedGroups::Loading => {
            lines.push(Line::from("Shared groups").fg(Color::DarkGray));
            lines.push(Line::from("Loading shared groups…"));
        }
        SharedGroups::Unavailable => {
            lines.push(Line::from("Shared groups").fg(Color::DarkGray));
            lines.push(Line::from("Shared groups unavailable"));
        }
        SharedGroups::Ready(groups) => {
            lines.push(Line::from(format!("Shared groups · {}", groups.len())).fg(Color::DarkGray));
            if groups.is_empty() {
                lines.push(Line::from("No known shared groups"));
            } else {
                for group in groups.iter().skip(scroll) {
                    lines.push(Line::from(truncate_with_ellipsis(&group.name, width)));
                }
            }
        }
    }
    let identity_y = photo.bottom().saturating_add(1).min(inner.bottom());
    let details = Rect::new(
        inner.x + 1.min(inner.width),
        identity_y,
        inner.width.saturating_sub(2),
        inner.bottom().saturating_sub(identity_y).saturating_sub(2),
    );
    frame.render_widget(Paragraph::new(lines), details);
    if inner.height >= 3 {
        let hint = if info.focused {
            "Known groups · j/k scroll · Esc close"
        } else {
            "Known groups · Space a i · Esc close"
        };
        frame.render_widget(
            Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
            Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        );
    }
}
