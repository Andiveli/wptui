// Status list pane for the Status section: one compact row per contact that
// has statuses, newest first. Unseen statuses are prefixed with a "●" marker.

use chrono::{DateTime, Datelike, Local};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{ListState, StatefulWidget},
};
use textwrap::core::display_width;
use whatsrust as wr;

use crate::app::App;

use super::contact_list::format_row;

/// Rows are one line high: the whole list is a flat contact picker.
pub const STATUS_ITEM_HEIGHT: usize = 1;
/// Unseen marker plus trailing space, rendered in front of the name.
pub const UNSEEN_MARKER: &str = "● ";
pub const SEEN_MARKER_PAD: &str = "  ";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusListItem {
    pub name: String,
    pub time: String,
    pub unseen: bool,
}

impl StatusListItem {
    pub fn from_contact(app: &App<'_>, contact: &wr::JID) -> Self {
        let name = app.contact_name(contact).to_string();
        let time = app
            .status_latest_time(contact)
            .map(|latest| format_status_time(latest, Local::now().timestamp()))
            .unwrap_or_default();
        let unseen = app.has_unseen_statuses(contact);
        Self { name, time, unseen }
    }
}

/// Selects the visible slice of a one-row-per-item list, mirroring
/// `contact_list::contact_viewport` with a row height of one.
pub fn status_viewport(selected: usize, offset: usize, height: u16, len: usize) -> (usize, usize) {
    let capacity = usize::from(height);
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

pub struct StatusList<'a> {
    items: &'a [StatusListItem],
}

impl<'a> StatusList<'a> {
    pub fn new(items: &'a [StatusListItem]) -> Self {
        Self { items }
    }
}

impl StatefulWidget for StatusList<'_> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.is_empty() || self.items.is_empty() {
            return;
        }
        let requested = state.selected().unwrap_or_default();
        let (selected, offset) =
            status_viewport(requested, state.offset(), area.height, self.items.len());
        state.select(Some(selected));
        *state.offset_mut() = offset;

        let capacity = usize::from(area.height);
        for (visible_index, item) in self.items.iter().skip(offset).take(capacity).enumerate() {
            let item_index = offset + visible_index;
            let y = area.y.saturating_add(visible_index as u16);
            let item_area = Rect::new(
                area.x,
                y,
                area.width,
                STATUS_ITEM_HEIGHT.min(area.bottom().saturating_sub(y) as usize) as u16,
            );
            if item_area.is_empty() {
                continue;
            }
            let base = if item_index == selected {
                Style::default().fg(Color::Green).bg(Color::DarkGray)
            } else {
                Style::default()
            };
            let marker = if item.unseen {
                UNSEEN_MARKER
            } else {
                SEEN_MARKER_PAD
            };
            let marker_width = display_width(marker);
            let text_x = area.x.saturating_add(marker_width as u16);
            let text_width = area.right().saturating_sub(text_x) as usize;
            let row = format_row(&item.name, Some(&item.time), text_width);
            let style = if item.unseen {
                base.add_modifier(Modifier::BOLD)
            } else {
                base
            };
            buf.set_stringn(text_x, y, &row, text_width, style);
            buf.set_stringn(area.x, y, marker, marker_width, base);
        }
    }
}

/// Relative time for recent statuses, calendar time for older ones:
/// "now", "5m ago", "2h ago", "Yesterday", "05 Jan", "2020 05 Jan".
pub fn format_status_time(timestamp: i64, now: i64) -> String {
    let diff = now.saturating_sub(timestamp);
    if diff < 60 {
        return "now".to_owned();
    }
    if diff < 3_600 {
        return format!("{}m ago", diff / 60);
    }
    if diff < 86_400 {
        return format!("{}h ago", diff / 3_600);
    }
    // `from_timestamp` lives on `DateTime<Utc>`; convert to local like
    // `contact_list::local_time` does.
    let status_date = DateTime::from_timestamp(timestamp, 0).map(DateTime::<Local>::from);
    let today = DateTime::from_timestamp(now, 0).map(DateTime::<Local>::from);
    match (status_date, today) {
        (Some(status), Some(today))
            if status.date_naive().succ_opt() == Some(today.date_naive()) =>
        {
            "Yesterday".to_owned()
        }
        (Some(status), Some(today)) if status.year() == today.year() => {
            status.format("%d %b").to_string()
        }
        (Some(status), _) => status.format("%Y %d %b").to_string(),
        _ => String::new(),
    }
}

// Status list pane for the Status section. Implemented in the GREEN pass;
// these tests pin the row formatting contract first (RED).

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeZone};

    use super::format_status_time;

    fn local_timestamp(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
        Local
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .expect("valid local timestamp")
            .timestamp()
    }

    #[test]
    fn relative_times_for_recent_statuses() {
        let now = local_timestamp(2025, 6, 1, 12, 0);
        assert_eq!(format_status_time(now - 10, now), "now");
        assert_eq!(format_status_time(now - 90, now), "1m ago");
        assert_eq!(format_status_time(now - 5_400, now), "1h ago");
        assert_eq!(format_status_time(now - 80_000, now), "22h ago");
    }

    #[test]
    fn calendar_times_for_older_statuses() {
        let now = local_timestamp(2025, 6, 1, 12, 0);
        assert_eq!(
            format_status_time(local_timestamp(2025, 5, 31, 12, 0), now),
            "Yesterday"
        );
        assert_eq!(
            format_status_time(local_timestamp(2025, 1, 5, 12, 0), now),
            "05 Jan"
        );
        assert_eq!(
            format_status_time(local_timestamp(2020, 1, 5, 12, 0), now),
            "2020 05 Jan"
        );
    }
}
