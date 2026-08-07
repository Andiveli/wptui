pub mod contact_list;
pub mod message_list;
pub mod status_list;
pub mod text_input;

use crate::app::App;
use crate::app::actions::{ConversationMode, FocusPane, PaneVisibility, Section};
use crate::app::composer::PendingAttachment;
use crate::app::contact_avatars::prioritized_avatar_requests;
use crate::app::events::{
    ViewerPreviewKey, ViewerPreviewState, ViewerStatus, viewer_preview_request,
};
use contact_list::{
    AVATAR_HEIGHT, AVATAR_WIDTH, CONTACT_ITEM_HEIGHT, ContactList, ContactListItem,
    contact_visible_range,
};
use message_list::{get_quoted_text, render_messages, render_status_messages};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Style, Stylize},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, StatefulWidget, Widget, Wrap},
};
use ratatui_image::{Resize, StatefulImage};
use status_list::{StatusList, StatusListItem};
use std::sync::Arc;
use tui_logger::TuiLoggerWidget;
use whatsrust as wr;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let content_area = if app.show_logs {
        let [content_area, logs_area] =
            Layout::horizontal([Constraint::Percentage(67), Constraint::Percentage(33)])
                .areas(frame.area());
        render_logs(frame, logs_area);
        content_area
    } else {
        frame.area()
    };
    let areas = navigation_areas(content_area, app.pane_visibility);

    if let Some(area) = areas.section_rail {
        render_section_rail(frame, app, area);
    }
    if app.rail_on_logout {
        app.contact_avatars.clear_window();
        if let Some(area) = areas.chat_list {
            render_structural_placeholder(frame, app, area);
        }
        render_logout_placeholder(frame, app, areas.conversation);
    } else if app.selected_section == Section::Chats {
        if let Some(area) = areas.chat_list {
            render_contacts(frame, app, area);
        } else {
            app.contact_avatars.clear_window();
        }
        render_chats(frame, app, areas.conversation);
    } else if app.selected_section == Section::Status {
        app.contact_avatars.clear_window();
        if let Some(area) = areas.chat_list {
            render_status_contacts(frame, app, area);
        }
        render_statuses(frame, app, areas.conversation);
    } else {
        app.contact_avatars.clear_window();
        if let Some(area) = areas.chat_list {
            render_structural_placeholder(frame, app, area);
        }
        render_structural_placeholder(frame, app, areas.conversation);
    }
    render_attachment_viewer(frame, app);
    render_url_picker(frame, app);
    render_share_picker(frame, app);
}

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

fn render_section_rail(frame: &mut Frame, app: &App, area: Rect) {
    let mut items = Section::ALL
        .iter()
        .map(|section| {
            ListItem::new(Section::title(*section)).style(Style::default().fg(Color::White))
        })
        .collect::<Vec<_>>();
    items.push(
        ListItem::new(crate::app::actions::LOGOUT_RAIL_TITLE)
            .style(Style::default().fg(Color::Red)),
    );
    let selected = if app.rail_on_logout {
        items.len() - 1
    } else {
        Section::ALL
            .iter()
            .position(|section| *section == app.selected_section)
            .unwrap_or(0)
    };
    let mut state = ratatui::widgets::ListState::default().with_selected(Some(selected));
    let list = List::new(items)
        .block(
            Block::bordered()
                .title("Sections")
                .border_style(
                    Style::default().fg(if app.focus_pane == FocusPane::SectionRail {
                        ratatui::style::Color::Green
                    } else {
                        ratatui::style::Color::White
                    }),
                ),
        )
        .highlight_symbol("> ")
        .highlight_style(if app.rail_on_logout {
            Style::default().red()
        } else {
            Style::default().green()
        });
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_structural_placeholder(frame: &mut Frame, app: &App, area: Rect) {
    let section = app.selected_section.title();
    frame.render_widget(
        Paragraph::new(format!("{section} is not available yet."))
            .block(Block::bordered().title(section)),
        area,
    );
}

fn render_logout_placeholder(frame: &mut Frame, app: &App, area: Rect) {
    let content = if app.logout_in_progress {
        "Logging out…\n\nThis removes the device from WhatsApp and clears the local session."
            .to_string()
    } else if app.pending_logout {
        // Reuse the message-menu interaction inside the pane: `>` marks the
        // selection, j/k move it, Enter confirms, Esc cancels (y/N also work).
        let menu_lines = ["Confirm logout", "Cancel"]
            .iter()
            .enumerate()
            .map(|(index, label)| {
                if index == app.logout_menu_index {
                    format!("> {label}")
                } else {
                    format!("  {label}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "This removes the device from WhatsApp and clears the local session.\n\n{menu_lines}\n\nj/k move · Enter confirms · Esc cancels"
        )
    } else {
        "Press Enter to sign out.\nThis removes the device from WhatsApp and clears the local session.".to_string()
    };
    frame.render_widget(
        Paragraph::new(content)
            .block(
                Block::bordered()
                    .title(crate::app::actions::LOGOUT_RAIL_TITLE)
                    .border_style(Style::default().fg(Color::Red)),
            )
            .style(Style::default().fg(Color::Red)),
        area,
    );
}

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
    let width = area.width.min(72).max(1);
    let height = area.height.min(16).max(1);
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

struct ComposerVisualLayout {
    rows: Vec<Vec<ComposerCell>>,
    logical_rows: Vec<(usize, usize)>,
    width: usize,
}

impl ComposerVisualLayout {
    fn text(&self) -> String {
        self.rows
            .iter()
            .map(|row| row.iter().map(|cell| cell.character).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn cursor(&self, cursor: (usize, usize)) -> (usize, usize) {
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

fn composer_visual_layout(lines: &[String], width: u16) -> ComposerVisualLayout {
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

fn render_logs(frame: &mut Frame, area: Rect) {
    let log_widget = TuiLoggerWidget::default()
        .style_trace(Style::new().dark_gray())
        .style_debug(Style::new().blue())
        .style_warn(Style::new().yellow())
        .style_error(Style::new().red().bold())
        .block(Block::default().title("Logs").borders(Borders::ALL));
    frame.render_widget(log_widget, area);
}

fn render_contacts(frame: &mut Frame, app: &mut App, area: Rect) {
    let chats = if app.contact_search.input.is_empty() {
        app.sorted_chats.clone()
    } else {
        app.filtered_chats.clone()
    };
    let items = chats
        .iter()
        .map(|chat| ContactListItem::from_chat(app, chat))
        .collect::<Vec<_>>();

    let mut list_area = area;
    if !app.contact_search.input.is_empty() || app.contact_search_active {
        let [search_area, new_list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Percentage(100)]).areas(area);
        list_area = new_list_area;

        let text = format!("/{}", app.contact_search.input);
        frame.render_widget(Paragraph::new(text), search_area);

        if app.contact_search_active {
            frame.set_cursor_position(Position::new(
                // Draw the cursor at the current position in the input field.
                // This position is can be controlled via the left and right arrow key
                search_area.x + app.contact_search.character_index as u16 + 1,
                // Move one line down, from the border to the input line
                search_area.y,
            ));
        }
    }

    let block = Block::bordered()
        .title(if let Some(p) = app.history_sync_percent {
            format!("Contacts ({p}%)")
        } else {
            "Contacts".to_string()
        })
        .border_style(
            Style::default().fg(if app.focus_pane == FocusPane::ChatList {
                ratatui::style::Color::Green
            } else {
                ratatui::style::Color::White
            }),
        );
    let contacts_area = block.inner(list_area);
    block.render(list_area, frame.buffer_mut());
    frame.render_stateful_widget(
        ContactList::new(&items),
        contacts_area,
        &mut app.chat_list_state,
    );

    let visible = contact_visible_range(
        app.chat_list_state.offset(),
        contacts_area.height,
        chats.len(),
    );
    app.contact_avatars.schedule(
        prioritized_avatar_requests(
            &chats,
            app.chat_list_state.selected(),
            visible.start,
            visible.len(),
        ),
        app.tx.clone(),
        Arc::clone(&app.picker),
    );
    for index in visible {
        let row = index.saturating_sub(app.chat_list_state.offset());
        let y = contacts_area
            .y
            .saturating_add((row * CONTACT_ITEM_HEIGHT) as u16);
        let avatar_area = Rect::new(
            contacts_area.x,
            y,
            AVATAR_WIDTH.min(contacts_area.width),
            AVATAR_HEIGHT.min(contacts_area.bottom().saturating_sub(y)),
        );
        // Partial Kitty placements can leave terminal artifacts after a scroll.
        if avatar_area.width == AVATAR_WIDTH
            && avatar_area.height == AVATAR_HEIGHT
            && let Some(protocol) = app.contact_avatars.protocol_mut(&chats[index])
        {
            StatefulImage::default().render(avatar_area, frame.buffer_mut(), protocol);
        }
    }
}

fn render_status_contacts(frame: &mut Frame, app: &mut App, area: Rect) {
    let items = app
        .status_contacts
        .iter()
        .map(|contact| StatusListItem::from_contact(app, contact))
        .collect::<Vec<_>>();

    let block = Block::bordered()
        .title("Status")
        .border_style(
            Style::default().fg(if app.focus_pane == FocusPane::ChatList {
                ratatui::style::Color::Green
            } else {
                ratatui::style::Color::White
            }),
        );
    let list_area = block.inner(area);
    block.render(area, frame.buffer_mut());

    if items.is_empty() {
        frame.render_widget(Paragraph::new("No statuses yet"), list_area);
        return;
    }
    frame.render_stateful_widget(
        StatusList::new(&items),
        list_area,
        &mut app.status_selection,
    );
}

fn render_statuses(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = app
        .open_status_contact()
        .map(|contact| app.contact_name(&contact).to_string())
        .unwrap_or_else(|| "Status".to_string());
    let border_color = if app.focus_pane == FocusPane::Conversation {
        ratatui::style::Color::Green
    } else {
        ratatui::style::Color::White
    };
    let block = Block::bordered()
        .title(title)
        .border_style(Style::default().fg(border_color));
    let content_area = block.inner(area);
    block.render(area, frame.buffer_mut());

    if app.open_status_contact().is_none() {
        frame.render_widget(
            Paragraph::new("Select a contact to view their statuses"),
            content_area,
        );
        return;
    }
    render_status_messages(frame, app, content_area);
}

const ANDIVELI_LOGO: [&str; 19] = [
    " ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢠⣆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣾⣿⡆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣼⣿⣿⣿⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣰⣿⠋⢿⣿⣷⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢠⣿⡏⠀⠘⣿⣿⣧⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣿⡿⠀⠀⠀⠹⣿⣿⣆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣾⣿⠃⠀⠀⠀⠀⢹⣿⣿⡄⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⣼⣿⠇⠀⠀⠀⠀⠀⠀⢻⣿⣷⡀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⣰⣿⡟⠀⠀⠀⠀⠀⠀⠀⠈⣿⣿⣧⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⢠⣿⣿⠁⠀⠀⠀⠀⠀⠀⠀⠀⠘⣿⣿⣧⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⣠⣿⣿⠇⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠹⣿⣿⣷⣄⠀⠀⠀⠀⠀",
    "⠈⣶⣦⣤⣶⣿⣿⣿⡏⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢹⣿⣿⣿⣿⣷⣶⣾⠇",
    "⠀⢻⣿⣿⣿⣿⣿⡿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢿⣿⣿⣿⣿⣿⣿⠀",
    "⠀⣼⣿⣿⣿⣿⣿⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⣿⣿⣿⣿⣿⣿⠀",
    "⠀⠛⠻⢿⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣸⣿⣿⡿⠟⠛⠃",
    "⠀⠀⠀⠀⠙⠿⣿⣧⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢠⣿⡿⠋⠀⠀⠀⠀",
    "⠀⠀⠑⠄⡀⠀⠈⠙⠳⢦⣀⠀⠀⠀⠀⠀⠀⠀⠀⢀⡠⠶⠋⠁⠀⢀⡠⠊⠀⠀",
    "⠀⠀⠀⠀⠈⠒⢤⣀⠀⠀⠀⠁⠀⠀⣠⣄⠀⠀⠀⠀⠀⠀⣀⡤⠒⠁⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠈⠉⠛⠷⠶⢶⣿⣿⣿⣿⡷⠶⠾⠛⠋⠁⠀⠀⠀⠀⠀⠀⠀",
];

/// Renders the Andiveli logo and product name in the empty chat panel
/// shown before any conversation is opened.
fn render_chat_empty_state(frame: &mut Frame, area: Rect) {
    let mut lines: Vec<Line<'static>> = ANDIVELI_LOGO
        .iter()
        .map(|line| Line::styled((*line).to_string(), Style::default().dark_gray()))
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "Andiveli",
        Style::default().fg(ratatui::style::Color::Green).bold(),
    ));

    let total = lines.len() as u16;
    if total >= area.height {
        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
        return;
    }
    let top_pad = (area.height - total) / 2;
    let [_, body, _] = Layout::vertical([
        Constraint::Length(top_pad),
        Constraint::Length(total),
        Constraint::Min(0),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), body);
}

pub fn render_chats(frame: &mut Frame, app: &mut App, area: Rect) {
    // Outer block wrapping messages + composer, like Concord's panel_block_owned
    let outer_title = app
        .open_chat()
        .map(|chat| app.contact_name(&chat))
        .unwrap_or_else(|| std::sync::Arc::from("Chat"));
    let outer_border_color = if app.focus_pane == FocusPane::Conversation {
        if app.conversation_mode == ConversationMode::ComposerEditing {
            ratatui::style::Color::Cyan
        } else {
            ratatui::style::Color::Green
        }
    } else {
        ratatui::style::Color::White
    };
    // Build the status string (action_notice, picker, menu) so we can drop it
    // on the right edge of the outer block. Keeping it here instead of painting
    // a separate Paragraph avoids it sticking around after it should disappear.
    let status = if app.logout_in_progress {
        Some("Logging out…".into())
    } else if app.pending_logout {
        // Reuse the top-right menu strip interaction (j/k + Enter/Esc),
        // mirroring the message menu format.
        Some(
            ["Confirm logout", "Cancel"]
                .iter()
                .enumerate()
                .map(|(index, label)| {
                    format!(
                        "{} {}",
                        if index == app.logout_menu_index {
                            ">"
                        } else {
                            " "
                        },
                        label
                    )
                })
                .collect::<Vec<_>>()
                .join(" | "),
        )
    } else if let Some((reactions, selected)) = &app.reaction_picker {
        Some(
            reactions
                .iter()
                .enumerate()
                .map(|(index, reaction)| {
                    format!(
                        "{} {} {}",
                        if index == *selected { ">" } else { " " },
                        reaction,
                        if index == *selected { "<" } else { " " }
                    )
                })
                .collect::<Vec<_>>()
                .join("  "),
        )
    } else if let Some((actions, selected)) = &app.message_menu {
        Some(
            actions
                .iter()
                .enumerate()
                .map(|(index, action)| {
                    format!(
                        "{} {:?}",
                        if index == *selected { ">" } else { " " },
                        action
                    )
                })
                .collect::<Vec<_>>()
                .join(" | "),
        )
    } else {
        app.action_notice.as_ref().map(|notice| match notice {
            crate::app::actions::ActionNotice::Forwarded {
                succeeded,
                failed,
                failure,
            } => {
                if *failed == 0 {
                    format!("Forwarded: {succeeded}")
                } else {
                    let reason = match failure {
                        whatsrust::ForwardFailure::SourceUnavailable => "Source unavailable",
                        whatsrust::ForwardFailure::InvalidSource => "Invalid source",
                        whatsrust::ForwardFailure::InvalidDestination => "Invalid destination",
                        whatsrust::ForwardFailure::SendFailed => "Send failed",
                        whatsrust::ForwardFailure::None => "Unknown failure",
                    };
                    format!("Forwarded: {succeeded} ok, {failed} failed ({reason})")
                }
            }
            crate::app::actions::ActionNotice::ReplyPrivatelyNamed(name) => {
                format!("Replying to {name} privately")
            }
            _ => format!("{notice:?}"),
        })
    };
    let selected_chat = app.open_chat();
    let marker = app
        .selected_presence
        .marker(selected_chat.as_ref(), crate::app::unix_now());
    let marker_span = marker.map(|marker| match marker {
        crate::app::presence::PresenceMarker::Online => Span::styled("●", Style::default().green()),
        crate::app::presence::PresenceMarker::RecentlyOffline => {
            Span::styled("●", Style::default().yellow())
        }
        crate::app::presence::PresenceMarker::Offline => Span::raw("○"),
    });
    let mut title = vec![Span::raw(" ")];
    if let Some(marker) = marker_span {
        title.extend([marker, Span::raw(" ")]);
    }
    title.extend([Span::raw(outer_title.to_string()), Span::raw(" ")]);
    let mut outer_block = Block::bordered()
        .title(Line::from(title))
        .border_style(Style::default().fg(outer_border_color));
    if let Some(status) = status {
        // Reserve room for the left title plus its surrounding spaces so the
        // right-aligned notice doesn't overlap it. Truncate with "…" when it
        // doesn't fit; the contact name wins over ephemeral status text.
        let marker_width = usize::from(marker.is_some()) * 2;
        let prefix_len =
            textwrap::core::display_width(format!(" {outer_title} ").as_str()) + marker_width;
        let available = (area.width as usize).saturating_sub(prefix_len + 2);
        let truncated = truncate_with_ellipsis(&status, available);
        if !truncated.is_empty() {
            outer_block = outer_block.title(Line::from(truncated).right_aligned());
        }
    }
    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    let composer_width = inner.width.saturating_sub(2);
    let composer_layout = composer_visual_layout(app.composer.input.lines(), composer_width);
    let input_cursor = app.composer.input.cursor();
    let composer_cursor = composer_layout.cursor((input_cursor.0, input_cursor.1));
    let composer_rows = composer_layout.rows.len().max(composer_cursor.0 + 1);
    let (chat_area, composer_area) = conversation_areas(
        inner,
        composer_rows,
        usize::from(app.composer.quote.is_some()),
        app.composer.pending.len(),
    );

    if app.open_chat().is_none() {
        render_chat_empty_state(frame, chat_area);
    } else {
        render_messages(frame, app, chat_area);
    }

    if let Some(chat_jid) = app.open_chat() {
        let border_color = if app.focus_pane == FocusPane::Conversation {
            if app.conversation_mode == ConversationMode::ComposerEditing {
                ratatui::style::Color::Cyan
            } else {
                ratatui::style::Color::Green
            }
        } else {
            ratatui::style::Color::White
        };
        let input_title = if app.conversation_mode == ConversationMode::EditingMessage {
            " Edit message (Enter save, Esc cancel) "
        } else {
            " Message input "
        };
        let input_block = Block::bordered()
            .title(input_title)
            .border_set(symbols::border::ROUNDED)
            .border_style(Style::default().fg(border_color));
        frame.render_widget(&input_block, composer_area);

        let mut input_area = input_block.inner(composer_area);

        // Show hint in the input area when not editing
        if app.conversation_mode == ConversationMode::MessageNavigation {
            let hint = format!(
                "Press i to write in {} | Space 1 sections | Space 2 chats",
                app.contact_name(&chat_jid)
            );
            frame.render_widget(Paragraph::new(hint).fg(border_color), input_area);
        } else {
            if let Some(msg) = &app.composer.quote {
                let [quote_area, input_areaa] =
                    Layout::vertical([Constraint::Length(1), Constraint::Percentage(100)])
                        .areas(input_area);

                input_area = input_areaa;

                frame.render_widget(
                    Paragraph::new(format!("> {}", get_quoted_text(msg))).dark_gray(),
                    quote_area,
                );
            }

            for preview in attachment_preview_lines(&app.composer.pending) {
                let [attach_area, input_areaa] =
                    Layout::vertical([Constraint::Length(1), Constraint::Percentage(100)])
                        .areas(input_area);

                input_area = input_areaa;

                frame.render_widget(
                    Paragraph::new(format!("🔗 {preview}")).dark_gray(),
                    attach_area,
                );
            }

            let cursor_row = composer_cursor.0;
            let scroll_top = cursor_row
                .saturating_add(1)
                .saturating_sub(input_area.height as usize);
            let text = if app.composer.input.lines().iter().all(String::is_empty) {
                app.composer.input.placeholder_text().to_owned()
            } else {
                composer_layout.text()
            };
            frame.render_widget(
                Paragraph::new(text).scroll((scroll_top.min(u16::MAX as usize) as u16, 0)),
                input_area,
            );
            if app.focus_pane == FocusPane::Conversation && !input_area.is_empty() {
                frame.set_cursor_position(composer_cursor_position(
                    input_area,
                    (cursor_row.saturating_sub(scroll_top), composer_cursor.1),
                ));
            }
        }
    }
}

fn render_attachment_viewer(frame: &mut Frame, app: &mut App) {
    let Some(viewer) = app.attachment_viewer.as_ref() else {
        return;
    };
    let layout = viewer_preview_layout(frame.area(), app.viewer_zoom);
    if layout.modal.is_empty() {
        return;
    }

    let hint = format!(
        "{:?}: {} [{}/{}] {:?} | zoom {}% | h/l arrows nav | -/+ zoom | x open | Esc close",
        viewer.kind,
        viewer.path,
        viewer.index + 1,
        viewer.attachment_count,
        viewer.status,
        app.viewer_zoom,
    );
    frame.render_widget(Clear, layout.modal);
    frame.render_widget(Block::bordered().title("Attachment viewer"), layout.modal);
    frame.render_widget(Paragraph::new(hint), layout.hint);

    if matches!(viewer.kind, wr::FileKind::Image | wr::FileKind::Video)
        && viewer.status != ViewerStatus::Failed
    {
        let key = ViewerPreviewKey::for_attachment(
            viewer.path.clone(),
            viewer.kind.clone(),
            layout.preview.width,
            layout.preview.height,
        );
        if let Some(key) = viewer_preview_request(&mut app.viewer_preview, key) {
            let _ = app.tx.send(crate::app::events::AppInput::App(
                crate::app::events::AppEvent::LoadViewerPreview(key),
            ));
        }
    }

    match app.viewer_preview.as_mut() {
        Some(ViewerPreviewState::Ready { key, protocol })
            if key.path == viewer.path
                && key.width == layout.preview.width
                && key.height == layout.preview.height =>
        {
            let render_rect = protocol.size_for(Resize::default(), layout.preview);
            let img_x =
                layout.preview.x + (layout.preview.width.saturating_sub(render_rect.width) / 2);
            let img_y =
                layout.preview.y + (layout.preview.height.saturating_sub(render_rect.height) / 2);
            let img_area = Rect::new(img_x, img_y, render_rect.width, render_rect.height);
            StatefulImage::default().render(img_area, frame.buffer_mut(), protocol.as_mut());
        }
        Some(ViewerPreviewState::Failed(_))
            if matches!(viewer.kind, wr::FileKind::Image | wr::FileKind::Video) =>
        {
            frame.render_widget(
                Paragraph::new(format!("Failed to load preview: {}", viewer.path)),
                layout.body,
            );
        }
        _ if matches!(viewer.kind, wr::FileKind::Image | wr::FileKind::Video) => {
            frame.render_widget(
                Paragraph::new(format!("Loading preview: {}", viewer.path)),
                layout.body,
            );
        }
        _ => frame.render_widget(
            Paragraph::new(format!("{:?}: {}", viewer.kind, viewer.path)),
            layout.body,
        ),
    }
}

fn render_share_picker(frame: &mut Frame, app: &mut App) {
    let Some(selected_count) = app
        .share_picker
        .as_ref()
        .map(|picker| picker.selected_count())
    else {
        return;
    };
    let modal = centered_modal_layout(frame.area());
    if modal.is_empty() {
        return;
    }
    let block = Block::bordered()
        .title(format!(" Forward message ({selected_count}) "))
        .border_set(symbols::border::ROUNDED);
    let inner = block.inner(modal);
    let [search_area, list_area, hint_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(inner);
    if let Some(picker) = app.share_picker.as_mut() {
        picker.set_viewport_height(list_area.height as usize);
    }
    let Some(picker) = app.share_picker.as_ref() else {
        return;
    };
    let visible = picker.visible_contacts();
    let viewport = picker.viewport();
    let items = visible[viewport.clone()]
        .iter()
        .enumerate()
        .map(|(row, jid)| {
            let index = viewport.start + row;
            let marker = if picker.is_selected(jid) {
                "[x]"
            } else {
                "[ ]"
            };
            let cursor = if index == picker.selected { ">" } else { " " };
            Line::from(format!("{cursor} {marker} {}", app.contact_name(jid)))
        });
    frame.render_widget(Clear, modal);
    frame.render_widget(block, modal);
    frame.render_widget(
        Paragraph::new(format!("Search: {}", picker.query)),
        search_area,
    );
    frame.render_widget(
        Paragraph::new(items.collect::<Vec<_>>()).wrap(Wrap { trim: true }),
        list_area,
    );
    frame.render_widget(
        Paragraph::new(
            "Type to search  ↑/↓ or j/k select  Space toggle  Enter forward  Esc cancel",
        )
        .dark_gray()
        .wrap(Wrap { trim: true }),
        hint_area,
    );
}

fn render_url_picker(frame: &mut Frame, app: &App) {
    let Some((urls, selected)) = app.url_picker.as_ref() else {
        return;
    };
    let modal = centered_modal_layout(frame.area());
    if modal.is_empty() {
        return;
    }
    let block = Block::bordered()
        .title(" Open link ")
        .border_set(symbols::border::ROUNDED);
    let inner = block.inner(modal);
    let [list_area, hint_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
    let items = urls.iter().enumerate().map(|(index, url)| {
        let marker = if index == *selected { "> " } else { "  " };
        Line::from(format!("{marker}{url}"))
    });
    frame.render_widget(Clear, modal);
    frame.render_widget(block, modal);
    frame.render_widget(
        Paragraph::new(items.collect::<Vec<_>>()).wrap(Wrap { trim: true }),
        list_area,
    );
    frame.render_widget(
        Paragraph::new("↑/↓ or j/k select  Enter open  Esc cancel")
            .dark_gray()
            .wrap(Wrap { trim: true }),
        hint_area,
    );
}

/// Truncate `value` to at most `width` terminal cells, appending `…` when
/// there is content left to cut. Returns `value` unchanged if it already fits.
/// Returns an empty string for `width <= 1` so callers can detect "no usable
/// space" without panicking.
fn truncate_with_ellipsis(value: &str, width: usize) -> String {
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
