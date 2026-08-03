use chrono::{DateTime, Local};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{ListState, StatefulWidget},
};
use textwrap::core::display_width;
use whatsrust as wr;

use crate::app::App;

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
    /// The current backend does not expose per-chat unread totals, so this remains absent.
    pub unread_count: Option<u32>,
}

impl ContactListItem {
    pub fn from_chat(app: &App<'_>, chat: &wr::JID) -> Self {
        let name = app.contact_name(chat).to_string();
        let latest = app
            .chat_messages
            .get(chat)
            .into_iter()
            .flatten()
            .filter_map(|id| app.messages.get(id))
            .max_by(|left, right| {
                left.info
                    .timestamp
                    .cmp(&right.info.timestamp)
                    .then_with(|| left.info.id.cmp(&right.info.id))
            });
        let timestamp = latest
            .map(|message| message.info.timestamp)
            .or_else(|| app.chats.get(chat).and_then(|chat| chat.last_message_time));

        Self {
            initials: initials(&name),
            name,
            preview: latest.map(message_preview).unwrap_or_default(),
            local_time: timestamp.and_then(local_time),
            unread_count: None,
        }
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
        let (selected, offset) =
            contact_viewport(requested, state.offset(), area.height, self.items.len());
        state.select(Some(selected));
        *state.offset_mut() = offset;

        let capacity = usize::from(area.height).div_ceil(CONTACT_ITEM_HEIGHT);
        for (visible_index, item) in self.items.iter().skip(offset).take(capacity).enumerate() {
            let item_index = offset + visible_index;
            let y = area
                .y
                .saturating_add((visible_index * CONTACT_ITEM_HEIGHT) as u16);
            let item_area = Rect::new(
                area.x,
                y,
                area.width,
                AVATAR_HEIGHT.min(area.bottom().saturating_sub(y)),
            );
            let base = if item_index == selected {
                Style::default().fg(Color::Green).bg(Color::DarkGray)
            } else {
                Style::default()
            };
            let name_style = base.add_modifier(Modifier::BOLD);
            buf.set_style(item_area, base);

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
            let unread = item
                .unread_count
                .filter(|count| *count > 0)
                .map(|count| count.to_string());
            let second = format_row(&item.preview, unread.as_deref(), text_width);
            if item_area.height > 0 {
                buf.set_stringn(text_x, y, first, text_width, name_style);
            }
            if item_area.height > 1 {
                buf.set_stringn(text_x, y.saturating_add(1), second, text_width, base);
            }
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
