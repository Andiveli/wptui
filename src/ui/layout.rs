use crate::app::actions::PaneVisibility;
use crate::app::composer::PendingAttachment;
use crate::app::preferences::ComposerDirection;
use crate::ui::bidi::{
    Direction, visual_graphemes_with_base_direction, visual_graphemes_with_ranges,
};
use ratatui::{
    layout::{Constraint, Layout, Position, Rect},
    widgets::Block,
};
use unicode_segmentation::UnicodeSegmentation;
use whatsrust as wr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NavigationAreas {
    pub section_rail: Option<Rect>,
    pub chat_list: Option<Rect>,
    pub conversation: Rect,
}

pub(crate) fn composer_viewport_width(
    area_width: u16,
    area_height: u16,
    visibility: PaneVisibility,
    show_logs: bool,
) -> u16 {
    let content_area = if show_logs {
        Layout::horizontal([Constraint::Percentage(67), Constraint::Percentage(33)])
            .areas::<2>(Rect::new(0, 0, area_width, area_height))[0]
    } else {
        Rect::new(0, 0, area_width, area_height)
    };
    navigation_areas(content_area, visibility)
        .conversation
        .width
        .saturating_sub(4)
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct ComposerCell {
    text: String,
    source_range: std::ops::Range<usize>,
    logical_column: usize,
    width: usize,
    direction: Direction,
    visual_row: usize,
    visual_column: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ComposerRow {
    cells: Vec<ComposerCell>,
    direction: Direction,
    alignment: ratatui::layout::Alignment,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum BoundaryAffinity {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HorizontalDirection {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ComposerCaret {
    pub(crate) logical_row: usize,
    pub(crate) logical_column: usize,
    pub(crate) visual_row: usize,
    pub(crate) visual_column: usize,
    pub(crate) affinity: BoundaryAffinity,
}

pub(crate) struct ComposerVisualLayout {
    rows: Vec<ComposerRow>,
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
            .map(|row| {
                row.cells
                    .iter()
                    .map(|cell| cell.text.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(crate) fn lines(&self) -> Vec<ratatui::text::Line<'static>> {
        self.rows
            .iter()
            .map(|row| {
                let text = row
                    .cells
                    .iter()
                    .map(|cell| cell.text.as_str())
                    .collect::<String>();
                ratatui::text::Line::from(text).alignment(row.alignment)
            })
            .collect()
    }

    fn legacy_cursor(&self, cursor: (usize, usize)) -> (usize, usize) {
        let logical_row = cursor.0.min(self.logical_rows.len().saturating_sub(1));
        let (first_row, row_count) = self.logical_rows[logical_row];
        let row_end = first_row + row_count;
        let line_direction = self.rows[first_row].direction;
        let logical_boundary = self.logical_boundary(logical_row, cursor.1);
        let mut candidates = Vec::new();

        for row_index in first_row..row_end {
            let row = &self.rows[row_index];
            for cell in &row.cells {
                let leading = if cell.direction == Direction::Rtl {
                    cell.visual_column + cell.width
                } else {
                    cell.visual_column
                };
                let trailing = if cell.direction == Direction::Rtl {
                    cell.visual_column
                } else {
                    cell.visual_column + cell.width
                };
                if cell.source_range.start == logical_boundary {
                    candidates.push((cell.visual_row, leading, BoundaryAffinity::Before));
                }
                if cell.source_range.end == logical_boundary {
                    candidates.push((cell.visual_row, trailing, BoundaryAffinity::After));
                }
            }
        }

        let preferred = if line_direction == Direction::Rtl {
            BoundaryAffinity::Before
        } else {
            BoundaryAffinity::After
        };
        let (row, column, _) = candidates
            .iter()
            .find(|candidate| candidate.2 == preferred)
            .copied()
            .or_else(|| candidates.first().copied())
            .unwrap_or_else(|| {
                let last_row = row_end.saturating_sub(1);
                (last_row, self.row_offset(&self.rows[last_row]), preferred)
            });
        if column >= self.width && line_direction == Direction::Ltr {
            (row + 1, 0)
        } else {
            (row, column.min(self.width.saturating_sub(1)))
        }
    }

    pub(crate) fn cursor(&self, cursor: (usize, usize)) -> (usize, usize) {
        self.cursor_with_affinity(cursor, None)
    }

    pub(crate) fn cursor_with_affinity(
        &self,
        cursor: (usize, usize),
        affinity: Option<BoundaryAffinity>,
    ) -> (usize, usize) {
        let caret = self.visual_caret(cursor, affinity);
        let row_width = self.rows[caret.visual_row]
            .cells
            .iter()
            .map(|cell| cell.width)
            .sum::<usize>();
        if caret.visual_column >= self.width
            && row_width <= self.width
            && self.rows[caret.visual_row].direction == Direction::Ltr
        {
            (caret.visual_row + 1, 0)
        } else {
            (
                caret.visual_row,
                caret.visual_column.min(self.width.saturating_sub(1)),
            )
        }
    }

    pub(crate) fn visual_caret(
        &self,
        cursor: (usize, usize),
        affinity: Option<BoundaryAffinity>,
    ) -> ComposerCaret {
        let logical_row = cursor.0.min(self.logical_rows.len().saturating_sub(1));
        let (first_row, row_count) = self.logical_rows[logical_row];
        let logical_column = self.normalize_logical_column(logical_row, cursor.1);
        let preferred = affinity.unwrap_or_else(|| {
            if self.rows[first_row].direction == Direction::Rtl {
                BoundaryAffinity::Before
            } else {
                BoundaryAffinity::After
            }
        });
        let carets = self.logical_carets(first_row..first_row + row_count);
        carets
            .iter()
            .find(|caret| caret.logical_column == logical_column && caret.affinity == preferred)
            .copied()
            .or_else(|| {
                carets
                    .iter()
                    .find(|caret| caret.logical_column == logical_column)
                    .copied()
            })
            .unwrap_or_else(|| {
                let row = first_row + row_count.saturating_sub(1);
                ComposerCaret {
                    logical_row,
                    logical_column,
                    visual_row: row,
                    visual_column: self.row_offset(&self.rows[row]),
                    affinity: preferred,
                }
            })
    }

    pub(crate) fn move_horizontal(
        &self,
        cursor: (usize, usize),
        affinity: Option<BoundaryAffinity>,
        direction: HorizontalDirection,
    ) -> ComposerCaret {
        let current = self.visual_caret(cursor, affinity);
        let carets = self.row_carets(current.visual_row..current.visual_row + 1);
        let current_index = carets
            .iter()
            .position(|caret| caret.visual_column == current.visual_column)
            .unwrap_or_else(|| match direction {
                HorizontalDirection::Left => carets.len(),
                HorizontalDirection::Right => 0,
            });
        let target = match direction {
            HorizontalDirection::Left => current_index
                .checked_sub(1)
                .and_then(|index| carets.get(index))
                .copied()
                .or_else(|| {
                    (current.visual_row > 0)
                        .then(|| self.row_carets(current.visual_row - 1..current.visual_row))
                        .and_then(|carets| carets.last().copied())
                }),
            HorizontalDirection::Right => carets.get(current_index + 1).copied().or_else(|| {
                (current.visual_row + 1 < self.rows.len())
                    .then(|| self.row_carets(current.visual_row + 1..current.visual_row + 2))
                    .and_then(|carets| carets.first().copied())
            }),
        };
        target.unwrap_or(current)
    }

    pub(crate) fn move_vertical(
        &self,
        cursor: (usize, usize),
        affinity: Option<BoundaryAffinity>,
        direction: HorizontalDirection,
        preferred_column: Option<usize>,
    ) -> ComposerCaret {
        let current = self.visual_caret(cursor, affinity);
        let target_row = match direction {
            HorizontalDirection::Left => current.visual_row.saturating_sub(1),
            HorizontalDirection::Right => {
                (current.visual_row + 1).min(self.rows.len().saturating_sub(1))
            }
        };
        let preferred = preferred_column.unwrap_or(current.visual_column);
        self.row_carets(target_row..target_row + 1)
            .into_iter()
            .min_by_key(|caret| (caret.visual_column.abs_diff(preferred), caret.visual_column))
            .unwrap_or(current)
    }

    fn logical_carets(&self, rows: std::ops::Range<usize>) -> Vec<ComposerCaret> {
        self.raw_row_carets(rows)
    }

    fn row_carets(&self, rows: std::ops::Range<usize>) -> Vec<ComposerCaret> {
        let carets = self.raw_row_carets(rows);
        let mut deduplicated: Vec<ComposerCaret> = Vec::with_capacity(carets.len());
        for caret in carets {
            if let Some(existing) = deduplicated.last_mut()
                && existing.visual_row == caret.visual_row
                && existing.visual_column == caret.visual_column
            {
                let preferred = if self.rows[caret.visual_row].direction == Direction::Rtl {
                    BoundaryAffinity::Before
                } else {
                    BoundaryAffinity::After
                };
                if caret.affinity == preferred {
                    *existing = caret;
                }
            } else {
                deduplicated.push(caret);
            }
        }
        deduplicated
    }

    fn raw_row_carets(&self, rows: std::ops::Range<usize>) -> Vec<ComposerCaret> {
        let mut carets = Vec::new();
        for row_index in rows {
            let row = &self.rows[row_index];
            if row.cells.is_empty() {
                carets.push(ComposerCaret {
                    logical_row: self.logical_row_for_visual(row_index),
                    logical_column: 0,
                    visual_row: row_index,
                    visual_column: self.row_offset(row),
                    affinity: BoundaryAffinity::Before,
                });
                continue;
            }
            let has_mixed_directions = row
                .cells
                .iter()
                .map(|cell| cell.direction)
                .any(|direction| direction != row.direction);
            for cell in &row.cells {
                let logical_row = self.logical_row_for_visual(row_index);
                let start = cell.logical_column;
                let end = start + cell.text.chars().count();
                // Neutral whitespace at a mixed boundary belongs to the
                // visual gap before the RTL run for caret purposes.
                let direction =
                    if has_mixed_directions && cell.text.chars().all(char::is_whitespace) {
                        Direction::Ltr
                    } else {
                        cell.direction
                    };
                if direction == Direction::Rtl {
                    carets.push(ComposerCaret {
                        logical_row,
                        logical_column: end,
                        visual_row: row_index,
                        visual_column: cell.visual_column,
                        affinity: BoundaryAffinity::After,
                    });
                    carets.push(ComposerCaret {
                        logical_row,
                        logical_column: start,
                        visual_row: row_index,
                        visual_column: cell.visual_column + cell.width,
                        affinity: BoundaryAffinity::Before,
                    });
                } else {
                    carets.push(ComposerCaret {
                        logical_row,
                        logical_column: start,
                        visual_row: row_index,
                        visual_column: cell.visual_column,
                        affinity: BoundaryAffinity::Before,
                    });
                    carets.push(ComposerCaret {
                        logical_row,
                        logical_column: end,
                        visual_row: row_index,
                        visual_column: cell.visual_column + cell.width,
                        affinity: BoundaryAffinity::After,
                    });
                }
            }
        }
        carets.sort_by_key(|caret| (caret.visual_row, caret.visual_column, caret.affinity));
        carets
    }

    fn logical_row_for_visual(&self, visual_row: usize) -> usize {
        self.logical_rows
            .iter()
            .position(|(first, count)| visual_row >= *first && visual_row < first + count)
            .unwrap_or(0)
    }

    fn normalize_logical_column(&self, logical_row: usize, logical_column: usize) -> usize {
        let (first_row, row_count) = self.logical_rows[logical_row];
        let mut cells = self.rows[first_row..first_row + row_count]
            .iter()
            .flat_map(|row| row.cells.iter())
            .collect::<Vec<_>>();
        cells.sort_by_key(|cell| cell.logical_column);
        let max = cells
            .last()
            .map(|cell| cell.logical_column + cell.text.chars().count())
            .unwrap_or(0);
        let column = logical_column.min(max);
        cells
            .into_iter()
            .find_map(|cell| {
                let end = cell.logical_column + cell.text.chars().count();
                (column > cell.logical_column && column < end).then_some(end)
            })
            .unwrap_or(column)
    }

    fn logical_boundary(&self, logical_row: usize, logical_column: usize) -> usize {
        let (first_row, row_count) = self.logical_rows[logical_row];
        let mut cells = self.rows[first_row..first_row + row_count]
            .iter()
            .flat_map(|row| row.cells.iter())
            .collect::<Vec<_>>();
        cells.sort_by_key(|cell| cell.logical_column);
        let max_column = cells
            .iter()
            .map(|cell| cell.logical_column + cell.text.chars().count())
            .max()
            .unwrap_or(0);
        let column = logical_column.min(max_column);
        let mut boundary = cells
            .first()
            .map(|cell| cell.source_range.start)
            .unwrap_or(0);
        for cell in cells {
            let start = cell.logical_column;
            let end = start + cell.text.chars().count();
            if column <= start {
                return cell.source_range.start;
            }
            if column < end {
                return cell.source_range.end;
            }
            boundary = cell.source_range.end;
        }
        boundary
    }

    fn row_offset(&self, row: &ComposerRow) -> usize {
        let row_width = row.cells.iter().map(|cell| cell.width).sum::<usize>();
        if row.alignment == ratatui::layout::Alignment::Right {
            self.width.saturating_sub(row_width)
        } else {
            0
        }
    }
}

pub(crate) fn composer_visual_layout(lines: &[String], width: u16) -> ComposerVisualLayout {
    composer_visual_layout_with_direction(lines, width, ComposerDirection::Auto)
}

pub(crate) fn composer_visual_layout_with_direction(
    lines: &[String],
    width: u16,
    composer_direction: ComposerDirection,
) -> ComposerVisualLayout {
    let mut rows = Vec::new();
    let mut logical_rows = Vec::new();
    for line in lines {
        let first_row = rows.len();
        let direction = match composer_direction {
            ComposerDirection::Auto => Direction::from_text(line),
            ComposerDirection::Rtl => Direction::Rtl,
        };
        let alignment = direction.alignment();
        for logical_cells in wrap_composer_line(line, width) {
            let cells = if logical_cells.is_empty() {
                Vec::new()
            } else {
                let start = logical_cells.first().unwrap().source_range.start;
                let end = logical_cells.last().unwrap().source_range.end;
                let visual = match composer_direction {
                    ComposerDirection::Auto => visual_graphemes_with_ranges(line, start..end),
                    ComposerDirection::Rtl => {
                        visual_graphemes_with_base_direction(line, start..end, direction)
                    }
                };
                visual
                    .into_iter()
                    .filter_map(|grapheme| {
                        logical_cells
                            .iter()
                            .find(|cell| cell.source_range == grapheme.source_range)
                            .cloned()
                            .map(|mut cell| {
                                cell.direction = grapheme.direction;
                                cell
                            })
                    })
                    .collect()
            };
            let visual_row = rows.len();
            let row_width = cells.iter().map(|cell| cell.width).sum::<usize>();
            let offset = if alignment == ratatui::layout::Alignment::Right {
                (width as usize).saturating_sub(row_width)
            } else {
                0
            };
            let mut visual_column = offset;
            let mut cells = cells;
            for cell in &mut cells {
                cell.visual_row = visual_row;
                cell.visual_column = visual_column;
                visual_column += cell.width;
            }
            rows.push(ComposerRow {
                cells,
                direction,
                alignment,
            });
        }
        logical_rows.push((first_row, rows.len() - first_row));
    }
    if rows.is_empty() {
        rows.push(ComposerRow {
            cells: Vec::new(),
            direction: Direction::Ltr,
            alignment: ratatui::layout::Alignment::Left,
        });
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
    let max_width = width as usize;
    let mut rows = Vec::new();
    let mut current: Vec<ComposerCell> = Vec::new();
    let mut current_width = 0;
    for (start, text) in line.grapheme_indices(true) {
        let cell = ComposerCell {
            text: text.to_owned(),
            source_range: start..start + text.len(),
            logical_column: line[..start].chars().count(),
            width: textwrap::core::display_width(text),
            direction: Direction::from_text(text),
            visual_row: 0,
            visual_column: 0,
        };
        if current_width + cell.width > max_width && !current.is_empty() {
            let break_at = current
                .iter()
                .rposition(|item| item.text.chars().all(char::is_whitespace));
            if let Some(index) = break_at {
                let next = current.split_off(index + 1);
                rows.push(std::mem::take(&mut current));
                current = next;
                current_width = current.iter().map(|item| item.width).sum();
            } else {
                rows.push(std::mem::take(&mut current));
                current_width = 0;
            }
        }
        if current_width + cell.width > max_width && !current.is_empty() {
            rows.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current_width += cell.width;
        current.push(cell);
    }
    rows.push(current);
    rows
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

    #[test]
    fn composer_preserves_visual_bidi_order_and_directional_alignment() {
        let rtl = composer_visual_layout(&["مرحبا".to_owned()], 8);
        assert_eq!(rtl.text(), "ابحرم");
        assert_eq!(
            rtl.lines()[0].alignment,
            Some(ratatui::layout::Alignment::Right)
        );
        assert_eq!(rtl.row_offset(&rtl.rows[0]), 3);

        let ltr = composer_visual_layout(&["hello".to_owned()], 8);
        assert_eq!(ltr.text(), "hello");
        assert_eq!(
            ltr.lines()[0].alignment,
            Some(ratatui::layout::Alignment::Left)
        );
        assert_eq!(ltr.row_offset(&ltr.rows[0]), 0);
    }

    #[test]
    fn explicit_rtl_override_forces_alignment_and_caret_plan_without_mutating_source() {
        let source = "hello 123".to_owned();
        let layout = composer_visual_layout_with_direction(
            std::slice::from_ref(&source),
            12,
            crate::app::preferences::ComposerDirection::Rtl,
        );
        assert_eq!(layout.text(), source);
        assert_eq!(
            layout.lines()[0].alignment,
            Some(ratatui::layout::Alignment::Right)
        );
        assert_eq!(layout.row_offset(&layout.rows[0]), 3);
        assert_ne!(
            layout.cursor((0, 0)),
            layout.cursor((0, source.chars().count()))
        );
        assert_eq!(source, "hello 123");
    }

    #[test]
    fn empty_explicit_rtl_layout_aligns_placeholder_with_caret() {
        let rtl = composer_visual_layout_with_direction(
            &[String::new()],
            12,
            crate::app::preferences::ComposerDirection::Rtl,
        );
        assert_eq!(
            rtl.lines()[0].alignment,
            Some(ratatui::layout::Alignment::Right)
        );
        assert_eq!(rtl.cursor((0, 0)), (0, 11));

        let auto = composer_visual_layout_with_direction(
            &[String::new()],
            12,
            crate::app::preferences::ComposerDirection::Auto,
        );
        assert_eq!(
            auto.lines()[0].alignment,
            Some(ratatui::layout::Alignment::Left)
        );
        assert_eq!(auto.cursor((0, 0)), (0, 0));
    }

    #[test]
    fn composer_handles_mixed_runs_without_changing_logical_source() {
        let source = "abc אבג 123".to_owned();
        let layout = composer_visual_layout(std::slice::from_ref(&source), 32);
        assert_eq!(layout.text(), "abc 123 גבא");
        assert_eq!(source, "abc אבג 123");
    }

    #[test]
    fn composer_keeps_paragraph_context_across_soft_wraps_and_explicit_lines() {
        let lines = vec!["abc אבג 123".to_owned(), "שלום".to_owned()];
        let layout = composer_visual_layout(&lines, 6);
        assert_eq!(layout.row_count(), 4);
        assert_eq!(layout.text().lines().count(), 4);
        assert_eq!(layout.rows[3].direction, Direction::Rtl);
    }

    #[test]
    fn mixed_bidi_boundaries_keep_explicit_affinity_deterministic() {
        let source = "אבג abc".to_owned();
        let layout = composer_visual_layout(std::slice::from_ref(&source), 20);
        let before = layout.visual_caret((0, 3), Some(BoundaryAffinity::Before));
        let after = layout.visual_caret((0, 3), Some(BoundaryAffinity::After));
        assert_eq!(before.affinity, BoundaryAffinity::Before);
        assert_eq!(after.affinity, BoundaryAffinity::After);
        assert_ne!(
            (before.visual_row, before.visual_column),
            (after.visual_row, after.visual_column)
        );
    }

    #[test]
    fn composer_keeps_graphemes_intact_at_narrow_widths() {
        let source = "e\u{301}".to_owned();
        let layout = composer_visual_layout(std::slice::from_ref(&source), 1);
        assert_eq!(layout.text(), source);
        let emoji = "👩‍💻".to_owned();
        assert_eq!(
            composer_visual_layout(std::slice::from_ref(&emoji), 2).text(),
            emoji
        );
        assert_eq!(composer_visual_rows(&["אב".to_owned()], 1), 2);
    }

    #[test]
    fn composer_cursor_mapping_is_deterministic_at_mixed_run_boundaries() {
        let lines = ["אבג abc".to_owned()];
        let first = composer_visual_cursor(&lines, (0, 3), 16);
        let second = composer_visual_cursor(&lines, (0, 3), 16);
        assert_eq!(first, second);
        assert!(first.1 < 16);
        assert_ne!(
            composer_visual_cursor(&lines, (0, 0), 16),
            composer_visual_cursor(&lines, (0, 7), 16)
        );
    }

    #[test]
    fn horizontal_caret_stops_have_one_stop_per_visual_boundary() {
        let layout = composer_visual_layout(&["abc".to_owned()], 12);
        let carets = layout.row_carets(0..1);
        for pair in carets.windows(2) {
            assert_ne!(
                (pair[0].visual_row, pair[0].visual_column),
                (pair[1].visual_row, pair[1].visual_column)
            );
        }
        let current = layout.visual_caret((0, 1), Some(BoundaryAffinity::After));
        let target = layout.move_horizontal(
            (0, 1),
            Some(BoundaryAffinity::After),
            HorizontalDirection::Right,
        );
        assert!(target.visual_column > current.visual_column);
    }

    #[test]
    fn vertical_soft_wrap_keeps_the_hard_line_as_the_logical_row() {
        let layout = composer_visual_layout(&["abcdef".to_owned()], 3);
        let target = layout.move_vertical((0, 6), None, HorizontalDirection::Right, Some(3));
        assert_eq!(target.logical_row, 0);
        assert_eq!(target.logical_column, 6);
    }
}
