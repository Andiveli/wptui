use chrono::{DateTime, Local};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{ListState, StatefulWidget},
};
use textwrap::core::display_width;
use whatsrust as wr;

use crate::app::{App, ChatRow};

pub const CONTACT_ITEM_HEIGHT: usize = 3;
pub const AVATAR_WIDTH: u16 = 4;
pub const AVATAR_HEIGHT: u16 = 2;
const AVATAR_GAP: u16 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactListItem {
    pub name: String,
    pub initials: String,
    pub preview: String,
    pub local_time: Option<String>,
}

impl ContactListItem {
    pub(crate) fn from_summary(
        row: &ChatRow,
        latest: Option<&wr::Message>,
        fallback_timestamp: Option<i64>,
        unread: usize,
    ) -> Self {
        let name = row.label.clone();
        Self {
            initials: initials(&name),
            name,
            preview: if unread > 0 {
                format!("{unread} unread")
            } else {
                latest.map(message_preview).unwrap_or_default()
            },
            local_time: latest
                .map(|message| message.info.timestamp)
                .or(fallback_timestamp)
                .and_then(local_time),
        }
    }

    pub fn from_chat(app: &App<'_>, chat: &wr::JID) -> Self {
        Self::from_row(
            app,
            &ChatRow {
                label: app.contact_name(chat).to_string(),
                members: vec![chat.clone()],
                target: chat.clone(),
            },
        )
    }

    pub fn from_row(app: &App<'_>, row: &ChatRow) -> Self {
        let name = row.label.clone();
        let latest = row
            .members
            .iter()
            .flat_map(|chat| app.chat_messages.get(chat).into_iter().flatten())
            .filter_map(|id| app.messages.get(id))
            .max_by(|left, right| {
                left.info
                    .timestamp
                    .cmp(&right.info.timestamp)
                    .then_with(|| left.info.id.cmp(&right.info.id))
            });
        let timestamp = latest.map(|message| message.info.timestamp).or_else(|| {
            row.members
                .iter()
                .filter_map(|jid| app.chats.get(jid).and_then(|chat| chat.last_message_time))
                .max()
        });

        Self {
            initials: initials(&name),
            name,
            preview: latest.map(message_preview).unwrap_or_default(),
            local_time: timestamp.and_then(local_time),
        }
    }

    pub fn from_contact_row(app: &App<'_>, row: &crate::app::ContactRow) -> Self {
        match row {
            crate::app::ContactRow::Chat(row) => {
                let mut item = Self::from_row(app, row);
                let unread = row
                    .members
                    .iter()
                    .map(|jid| app.pending_new_messages(jid))
                    .sum::<usize>();
                if unread > 0 {
                    item.preview = format!("{unread} unread");
                }
                item
            }
            crate::app::ContactRow::VirtualAnnouncement(row) => {
                let mut item = Self::from_row(app, row);
                item.initials = "📢".into();
                item
            }
            crate::app::ContactRow::Available {
                name,
                participant_count,
                ..
            } => Self {
                name: name.clone(),
                initials: initials(name),
                preview: participant_count
                    .map(|count| format!("{count} members"))
                    .unwrap_or_default(),
                local_time: None,
            },
            crate::app::ContactRow::Header(name) | crate::app::ContactRow::Action(name) => Self {
                name: name.clone(),
                initials: String::new(),
                preview: String::new(),
                local_time: None,
            },
        }
    }
}

fn row_height(item: &ContactListItem) -> usize {
    if item.initials.is_empty()
        && item.preview.is_empty()
        && matches!(
            item.name.as_str(),
            "Groups you're in" | "Groups you can join"
        )
    {
        1
    } else {
        CONTACT_ITEM_HEIGHT
    }
}

pub fn initials(name: &str) -> String {
    let words = name.split_whitespace().collect::<Vec<_>>();
    let selected = match words.as_slice() {
        [] => return String::new(),
        [word] => vec![*word],
        words => vec![words[0], words[words.len() - 1]],
    };

    let value = selected
        .into_iter()
        .filter_map(|word| word.chars().next())
        .flat_map(char::to_uppercase)
        .collect::<String>();
    truncate(&value, AVATAR_WIDTH as usize)
}

pub fn truncate(value: &str, width: usize) -> String {
    let mut result = String::new();
    for character in value.chars() {
        let candidate = format!("{result}{character}");
        if display_width(&candidate) > width {
            break;
        }
        result.push(character);
    }
    result
}

pub fn format_row(left: &str, right: Option<&str>, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let right = right.unwrap_or_default();
    let right = truncate(right, width);
    let right_width = display_width(&right);
    let gap = usize::from(!right.is_empty() && right_width < width);
    let left_width = width.saturating_sub(right_width).saturating_sub(gap);
    let left = truncate(left, left_width);
    let padding = width
        .saturating_sub(display_width(&left))
        .saturating_sub(right_width);
    format!("{left}{}{right}", " ".repeat(padding))
}

pub fn contact_viewport(selected: usize, offset: usize, height: u16, len: usize) -> (usize, usize) {
    let capacity = usize::from(height) / CONTACT_ITEM_HEIGHT;
    if capacity == 0 || len == 0 {
        return (selected.min(len.saturating_sub(1)), 0);
    }
    let selected = selected.min(len - 1);
    let max_offset = len.saturating_sub(capacity);
    let mut offset = offset.min(max_offset);
    if selected < offset {
        offset = selected;
    } else if selected >= offset.saturating_add(capacity) {
        offset = selected.saturating_add(1).saturating_sub(capacity);
    }
    (selected, offset)
}

pub fn contact_visible_range(offset: usize, height: u16, len: usize) -> std::ops::Range<usize> {
    let count = usize::from(height).div_ceil(CONTACT_ITEM_HEIGHT);
    offset.min(len)..offset.saturating_add(count).min(len)
}

pub fn visible_contact_rows(
    items: &[ContactListItem],
    offset: usize,
    height: u16,
) -> Vec<(usize, u16)> {
    let mut y = 0u16;
    let mut rows = Vec::new();
    for (index, item) in items.iter().enumerate().skip(offset) {
        if y >= height {
            break;
        }
        rows.push((index, y));
        y = y.saturating_add(row_height(item) as u16);
    }
    rows
}

pub struct ContactList<'a> {
    items: &'a [ContactListItem],
}

impl<'a> ContactList<'a> {
    pub fn new(items: &'a [ContactListItem]) -> Self {
        Self { items }
    }
}

impl StatefulWidget for ContactList<'_> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.is_empty() || self.items.is_empty() {
            return;
        }
        let requested = state.selected().unwrap_or_default();
        let selected = requested.min(self.items.len().saturating_sub(1));
        let mut offset = state.offset().min(selected);
        loop {
            let used = self.items[offset..=selected]
                .iter()
                .map(row_height)
                .sum::<usize>();
            if used <= usize::from(area.height) || offset == selected {
                break;
            }
            offset += 1;
        }
        while selected < offset {
            offset = selected;
        }
        while self.items[offset..=selected]
            .iter()
            .map(row_height)
            .sum::<usize>()
            > usize::from(area.height)
            && selected > offset
        {
            offset += 1;
        }
        state.select(Some(selected));
        *state.offset_mut() = offset;

        let mut y = area.y;
        for (item_index, item) in self.items.iter().enumerate().skip(offset) {
            if y >= area.bottom() {
                break;
            }
            let item_area = Rect::new(
                area.x,
                y,
                area.width,
                row_height(item)
                    .min(usize::from(AVATAR_HEIGHT))
                    .min(area.bottom().saturating_sub(y) as usize) as u16,
            );
            let base = if item_index == selected {
                Style::default().fg(Color::Green).bg(Color::DarkGray)
            } else {
                Style::default()
            };
            let name_style = base.add_modifier(Modifier::BOLD);
            buf.set_style(item_area, base);

            if row_height(item) == 1 {
                buf.set_stringn(area.x, y, &item.name, area.width as usize, name_style);
                y = y.saturating_add(1);
                continue;
            }

            let avatar_width = AVATAR_WIDTH.min(item_area.width);
            let initials = truncate(&item.initials, avatar_width as usize);
            let initials_x = item_area
                .x
                .saturating_add(avatar_width.saturating_sub(display_width(&initials) as u16) / 2);
            if item_area.height > 0 {
                buf.set_stringn(
                    initials_x,
                    item_area.y,
                    &initials,
                    avatar_width as usize,
                    base,
                );
            }

            let text_x = area
                .x
                .saturating_add(AVATAR_WIDTH.saturating_add(AVATAR_GAP).min(area.width));
            let text_width = area.right().saturating_sub(text_x) as usize;
            let first = format_row(&item.name, item.local_time.as_deref(), text_width);
            let second = format_row(&item.preview, None, text_width);
            if item_area.height > 0 {
                buf.set_stringn(text_x, y, first, text_width, name_style);
            }
            if item_area.height > 1 {
                buf.set_stringn(text_x, y.saturating_add(1), second, text_width, base);
            }
            y = y.saturating_add(row_height(item) as u16);
        }
    }
}

fn local_time(timestamp: i64) -> Option<String> {
    DateTime::from_timestamp(timestamp, 0)
        .map(|time| DateTime::<Local>::from(time).format("%H:%M").to_string())
}

fn message_preview(message: &wr::Message) -> String {
    match &message.message {
        wr::MessageContent::Text(text) => text.to_string(),
        wr::MessageContent::File(file) => file
            .caption
            .as_deref()
            .map(str::to_owned)
            .unwrap_or_else(|| {
                match file.kind {
                    wr::FileKind::Image => "Image",
                    wr::FileKind::Video => "Video",
                    wr::FileKind::Audio => "Audio",
                    wr::FileKind::Document => "Document",
                    wr::FileKind::Sticker => "Sticker",
                }
                .to_owned()
            }),
    }
}
