use crate::app::actions::PaneVisibility;
use crate::app::composer::PendingAttachment;
use ratatui::{
    layout::{Constraint, Layout, Position, Rect},
    widgets::Block,
};
use whatsrust as wr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NavigationAreas {
    pub section_rail: Option<Rect>,
    pub chat_list: Option<Rect>,
    pub conversation: Rect,
}

pub fn navigation_areas(area: Rect, visibility: PaneVisibility) -> NavigationAreas {
    let rail_width = if visibility.section_rail {
        14.min(area.width)
    } else {
        0
    };
    let remaining = area.width.saturating_sub(rail_width);
    let chat_width = if visibility.chat_list {
        30.min(remaining)
    } else {
        0
    };
    let conversation_width = remaining.saturating_sub(chat_width);
    let rail = Rect::new(area.x, area.y, rail_width, area.height);
    let chat = Rect::new(
        area.x.saturating_add(rail_width),
        area.y,
        chat_width,
        area.height,
    );
    let conversation = Rect::new(
        chat.x.saturating_add(chat_width),
        area.y,
        conversation_width,
        area.height,
    );

    NavigationAreas {
        section_rail: visibility.section_rail.then_some(rail),
        chat_list: visibility.chat_list.then_some(chat),
        conversation,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewerPreviewLayout {
    pub modal: Rect,
    pub body: Rect,
    pub hint: Rect,
    pub preview: Rect,
}

pub fn centered_modal_layout(area: Rect) -> Rect {
    if area.is_empty() {
        return area;
    }
    let width = area.width.clamp(1, 72);
    let height = area.height.clamp(1, 16);
    Rect::new(
        area.x.saturating_add(area.width.saturating_sub(width) / 2),
        area.y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    )
}

pub fn viewer_preview_layout(area: Rect, zoom_percent: u16) -> ViewerPreviewLayout {
    let modal = Rect::new(
        area.x.saturating_add(2),
        area.y.saturating_add(2),
        area.width.saturating_sub(4),
        area.height.saturating_sub(4),
    );
    let inner = Block::bordered().inner(modal);
    let [body, hint] = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
    let zoom_factor = (zoom_percent as f32 / 100.0).clamp(0.25, 4.0);
    let pct = (85.0_f32 * zoom_factor).clamp(20.0, 100.0) / 100.0;
    let width = ((body.width as f32) * pct).round() as u16;
    let height = ((body.height as f32) * pct).round() as u16;
    let preview = Rect::new(
        body.x.saturating_add(body.width.saturating_sub(width) / 2),
        body.y
            .saturating_add(body.height.saturating_sub(height) / 2),
        width,
        height,
    );
    ViewerPreviewLayout {
        modal,
        body,
        hint,
        preview,
    }
}

pub fn composer_cursor_position(input_area: Rect, cursor: (usize, usize)) -> Position {
    let (row, column) = cursor;
    Position::new(input_area.x + column as u16, input_area.y + row as u16)
}

pub fn composer_visual_rows(lines: &[String], width: u16) -> usize {
    composer_visual_layout(lines, width).rows.len()
}

pub fn composer_visual_cursor(
    lines: &[String],
    cursor: (usize, usize),
    width: u16,
) -> (usize, usize) {
    composer_visual_layout(lines, width).cursor(cursor)
}

#[derive(Clone, Copy)]
struct ComposerCell {
    character: char,
    logical_column: usize,
    width: usize,
}

pub(crate) struct ComposerVisualLayout {
    rows: Vec<Vec<ComposerCell>>,
    logical_rows: Vec<(usize, usize)>,
    width: usize,
}

impl ComposerVisualLayout {
    pub(crate) fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn text(&self) -> String {
        self.rows
            .iter()
            .map(|row| row.iter().map(|cell| cell.character).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(crate) fn cursor(&self, cursor: (usize, usize)) -> (usize, usize) {
        let logical_row = cursor.0.min(self.logical_rows.len().saturating_sub(1));
        let logical_column = cursor.1;
        let (first_row, row_count) = self.logical_rows[logical_row];

        for (row_offset, row) in self.rows[first_row..first_row + row_count]
            .iter()
            .enumerate()
        {
            let mut column = 0;
            for cell in row {
                if cell.logical_column >= logical_column {
                    return (first_row + row_offset, column);
                }
                column += cell.width;
            }
        }

        let last_row = first_row + row_count - 1;
        let column = self.rows[last_row].iter().map(|cell| cell.width).sum();
        if column >= self.width {
            (last_row + 1, 0)
        } else {
            (last_row, column)
        }
    }
}

pub(crate) fn composer_visual_layout(lines: &[String], width: u16) -> ComposerVisualLayout {
    let mut rows = Vec::new();
    let mut logical_rows = Vec::new();

    for line in lines {
        let first_row = rows.len();
        let wrapped = wrap_composer_line(line, width);
        let row_count = wrapped.len();
        rows.extend(wrapped);
        logical_rows.push((first_row, row_count));
    }

    if rows.is_empty() {
        rows.push(Vec::new());
        logical_rows.push((0, 1));
    }

    ComposerVisualLayout {
        rows,
        logical_rows,
        width: width as usize,
    }
}

fn wrap_composer_line(line: &str, width: u16) -> Vec<Vec<ComposerCell>> {
    if width == 0 {
        return vec![Vec::new()];
    }

    // This mirrors Ratatui's WordWrapper with `trim: false`, which Paragraph used
    // before the composer switched to precomputed visual rows.
    let max_width = width as usize;
    let mut rows = Vec::new();
    let mut pending_line: Vec<ComposerCell> = Vec::new();
    let mut pending_word = Vec::new();
    let mut pending_whitespace = Vec::new();
    let mut line_width = 0;
    let mut word_width = 0;
    let mut whitespace_width = 0;
    let mut non_whitespace_previous = false;

    for (logical_column, character) in line.chars().enumerate() {
        let cell = ComposerCell {
            character,
            logical_column,
            width: display_width(character),
        };
        if cell.width > max_width {
            continue;
        }

        let is_whitespace = character.is_whitespace();
        let word_found = non_whitespace_previous && is_whitespace;
        let untrimmed_overflow =
            pending_line.is_empty() && word_width + whitespace_width + cell.width > max_width;

        if word_found || untrimmed_overflow {
            pending_line.append(&mut pending_whitespace);
            line_width += whitespace_width;
            pending_line.append(&mut pending_word);
            line_width += word_width;
            whitespace_width = 0;
            word_width = 0;
        }

        let line_full = line_width >= max_width;
        let pending_word_overflow =
            cell.width > 0 && line_width + whitespace_width + word_width >= max_width;
        if line_full || pending_word_overflow {
            let mut remaining_width = max_width.saturating_sub(line_width);
            rows.push(std::mem::take(&mut pending_line));
            line_width = 0;

            while let Some(whitespace) = pending_whitespace.first() {
                if whitespace.width > remaining_width {
                    break;
                }
                whitespace_width -= whitespace.width;
                remaining_width -= whitespace.width;
                pending_whitespace.remove(0);
            }

            if is_whitespace && pending_whitespace.is_empty() {
                continue;
            }
        }

        if is_whitespace {
            whitespace_width += cell.width;
            pending_whitespace.push(cell);
        } else {
            word_width += cell.width;
            pending_word.push(cell);
        }
        non_whitespace_previous = !is_whitespace;
    }

    pending_line.append(&mut pending_whitespace);
    pending_line.append(&mut pending_word);
    if pending_line.is_empty() {
        rows.push(Vec::new());
    } else {
        rows.push(pending_line);
    }
    rows
}

fn display_width(character: char) -> usize {
    let mut buffer = [0; 4];
    textwrap::core::display_width(character.encode_utf8(&mut buffer))
}

pub fn composer_height(
    terminal_height: u16,
    input_lines: usize,
    quote_rows: usize,
    attachment_rows: usize,
) -> u16 {
    let desired = 2_u16
        .saturating_add(input_lines.max(1) as u16)
        .saturating_add(quote_rows as u16)
        .saturating_add(attachment_rows as u16);
    desired
        .min(12)
        .min(terminal_height.saturating_sub(1))
        .max(1)
}

pub fn conversation_areas(
    area: Rect,
    input_lines: usize,
    quote_rows: usize,
    attachment_rows: usize,
) -> (Rect, Rect) {
    let composer_height = composer_height(area.height, input_lines, quote_rows, attachment_rows);
    let [messages, _gap, composer] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(composer_height),
    ])
    .areas(area);
    (messages, composer)
}

/// Returns a bounded floating picker rectangle directly above the composer.
/// The rectangle is deliberately outside `composer` so the input's viewport
/// cannot clip or repaint the suggestions.
pub fn composer_mention_picker_area(
    conversation: Rect,
    composer: Rect,
    candidate_count: usize,
    widest_candidate: usize,
) -> Option<Rect> {
    if candidate_count == 0 || conversation.width == 0 || composer.width == 0 {
        return None;
    }

    let desired_height = candidate_count.min(6).saturating_add(2) as u16;
    let available_above = composer.y.saturating_sub(conversation.y);
    let height = desired_height.min(available_above);
    if height < 3 {
        return None;
    }

    let desired_width = widest_candidate.saturating_add(4).clamp(12, 48) as u16;
    let width = desired_width.min(composer.width).min(conversation.width);
    if width < 3 {
        return None;
    }
    let right = conversation.x.saturating_add(conversation.width);
    let max_x = right.saturating_sub(width);
    let x = composer.x.clamp(conversation.x, max_x);
    Some(Rect {
        x,
        y: composer.y.saturating_sub(height),
        width,
        height,
    })
}

pub fn attachment_preview_lines(attachments: &[PendingAttachment]) -> Vec<String> {
    attachments
        .iter()
        .map(|attachment| {
            let kind = match attachment.kind {
                wr::FileKind::Image => "Image",
                wr::FileKind::Video => "Video",
                wr::FileKind::Audio => "Audio",
                wr::FileKind::Document => "Document",
                wr::FileKind::Sticker => "Sticker",
            };
            format!("{kind}: {}", attachment.display_name())
        })
        .collect()
}

pub(crate) fn truncate_with_ellipsis(value: &str, width: usize) -> String {
    if width <= 1 || value.is_empty() {
        return String::new();
    }
    if textwrap::core::display_width(value) <= width {
        return value.to_owned();
    }
    let ellipsis_cost = textwrap::core::display_width("…");
    let budget = width.saturating_sub(ellipsis_cost);
    let mut result = String::new();
    for ch in value.chars() {
        let next = format!("{result}{ch}");
        if textwrap::core::display_width(&next) > budget {
            break;
        }
        result = next;
    }
    result.push('…');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_module_exposes_navigation_and_modal_contracts() {
        let areas = navigation_areas(Rect::new(0, 0, 100, 20), PaneVisibility::default());
        assert_eq!(areas.conversation, Rect::new(44, 0, 56, 20));
        assert_eq!(
            centered_modal_layout(Rect::new(0, 0, 100, 40)),
            Rect::new(14, 12, 72, 16)
        );
        assert_eq!(
            composer_cursor_position(Rect::new(2, 3, 10, 4), (1, 2)),
            Position::new(4, 4)
        );
    }

    #[test]
    fn mention_picker_is_outside_composer_and_bounded_above_it() {
        let conversation = Rect::new(10, 2, 50, 18);
        let composer = Rect::new(10, 15, 50, 5);
        let picker = composer_mention_picker_area(conversation, composer, 20, 80).unwrap();

        assert!(picker.bottom() <= composer.y);
        assert!(picker.y >= conversation.y);
        assert!(picker.right() <= conversation.right());
        assert_eq!(picker.height, 8);
        assert!(picker.width <= 48);
    }

    #[test]
    fn mention_picker_is_omitted_when_a_short_terminal_has_no_room() {
        assert_eq!(
            composer_mention_picker_area(Rect::new(0, 0, 30, 2), Rect::new(0, 2, 30, 1), 3, 10,),
            None
        );
    }
}
