pub mod contact_list;
mod contacts;
mod layout;
pub mod message_list;
mod navigation;
pub mod status_list;
pub mod text_input;

pub use layout::{
    NavigationAreas, ViewerPreviewLayout, attachment_preview_lines, centered_modal_layout,
    composer_cursor_position, composer_height, composer_visual_cursor, composer_visual_rows,
    conversation_areas, navigation_areas, viewer_preview_layout,
};
pub(crate) use layout::{composer_visual_layout, truncate_with_ellipsis};

use crate::app::App;
use crate::app::actions::{ConversationMode, FocusPane, Section};
use crate::app::events::{
    ViewerPreviewKey, ViewerPreviewState, ViewerStatus, viewer_preview_request,
};
use contacts::render_contacts;
use message_list::{get_quoted_text, render_messages, render_status_messages};
use navigation::{
    render_logout_placeholder, render_logs, render_section_rail, render_structural_placeholder,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Style, Stylize},
    symbols,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph, StatefulWidget, Widget, Wrap},
};
use ratatui_image::{Resize, StatefulImage};
use status_list::{StatusList, StatusListItem};
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
    render_file_picker(frame, app);
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
    let composer_rows = composer_layout.row_count().max(composer_cursor.0 + 1);
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
                "Press i to write in {} (Ctrl+O attach) | Space 1 sections | Space 2 chats",
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

fn render_file_picker(frame: &mut Frame, app: &mut App) {
    let Some(picker) = app.file_picker.as_mut() else {
        return;
    };
    let modal = centered_modal_layout(frame.area());
    if modal.is_empty() {
        return;
    }
    let block = Block::bordered()
        .title(format!(" Attach file: {} ", picker.current_dir().display()))
        .border_set(symbols::border::ROUNDED);
    let inner = block.inner(modal);
    let [search_area, list_area, hint_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(inner);
    picker.set_viewport_height(list_area.height as usize);
    let visible = picker.visible_entries();
    let viewport = picker.viewport();
    let items = visible[viewport.clone()]
        .iter()
        .enumerate()
        .map(|(row, entry)| {
            let index = viewport.start + row;
            let cursor = if index == picker.selected { ">" } else { " " };
            let mark = if !entry.is_dir && picker.is_selected(&entry.path) {
                "[x]"
            } else {
                "   "
            };
            Line::from(format!("{cursor}{mark} {}", entry.display_name()))
        });
    frame.render_widget(Clear, modal);
    frame.render_widget(block, modal);
    let filter_line = if picker.searching {
        format!("/{}_", picker.query)
    } else {
        format!("Filter: {}", picker.query)
    };
    frame.render_widget(
        Paragraph::new(format!(
            "{filter_line}  ({} selected)",
            picker.selected_count()
        ))
        .dark_gray(),
        search_area,
    );
    frame.render_widget(
        Paragraph::new(items.collect::<Vec<_>>()).wrap(Wrap { trim: true }),
        list_area,
    );
    frame.render_widget(
        Paragraph::new("j/k select  h/← up  l/→ open  Spc mark  Enter attach  / search  Esc close")
            .dark_gray()
            .wrap(Wrap { trim: true }),
        hint_area,
    );
}
