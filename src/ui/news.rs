use crate::app::{App, actions::FocusPane};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
};

pub(crate) const NEWS_COLOR: Color = Color::Rgb(139, 160, 180);

// Hard-wrap on grapheme boundaries. Explicit rows make the whole bounded
// backend body scrollable, without Paragraph's u16 scroll-offset ceiling.
fn wrapped_lines(text: &str, width: u16) -> Vec<Line<'static>> {
    use unicode_segmentation::UnicodeSegmentation;
    if width == 0 {
        return vec![];
    }
    let mut rows = Vec::new();
    for logical in text.split('\n') {
        let mut row = String::new();
        let mut cells = 0;
        let expanded = logical.replace('\t', "    ");
        for grapheme in expanded.graphemes(true) {
            let size = Line::from(grapheme).width();
            if cells + size > usize::from(width) && !row.is_empty() {
                rows.push(Line::from(std::mem::take(&mut row)));
                cells = 0;
            }
            row.push_str(grapheme);
            cells += size;
        }
        rows.push(Line::from(row));
    }
    rows
}

pub(super) fn rail_title(inner_width: u16) -> &'static str {
    // Reserve the same two cells used by List's selection marker.
    if inner_width >= 12 {
        "News wptui"
    } else {
        "News"
    }
}

pub(super) fn render(frame: &mut Frame, app: &mut App, list_area: Option<Rect>, body_area: Rect) {
    if let Some(area) = list_area {
        let [installed_area, releases_area] = Layout::vertical([
            Constraint::Length(8.min(area.height / 2)),
            Constraint::Min(0),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new(format!(
                "Installed {}\n{}",
                app.news.installed.version, app.news.installed.body
            ))
            .wrap(Wrap { trim: false })
            .block(
                Block::bordered()
                    .title("Installed notes")
                    .border_style(Style::default().fg(NEWS_COLOR)),
            ),
            installed_area,
        );
        let mut items = vec![ListItem::new(format!(
            "Installed {}",
            app.news.installed.version
        ))];
        if let Some(history) = &app.news.history {
            items.extend(history.releases.iter().map(|note| {
                ListItem::new(format!(
                    "{}  {}",
                    note.version,
                    note.published_at.as_deref().unwrap_or("date unavailable")
                ))
            }));
        }
        let mut state = ListState::default().with_selected(Some(app.news.selected));
        frame.render_stateful_widget(
            List::new(items)
                .block(Block::bordered().title("Stable releases").border_style(
                    Style::default().fg(if app.focus_pane == FocusPane::ChatList {
                        Color::Green
                    } else {
                        NEWS_COLOR
                    }),
                ))
                .highlight_symbol("> ")
                .highlight_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(NEWS_COLOR)
                        .add_modifier(Modifier::BOLD),
                ),
            releases_area,
            &mut state,
        );
        let inner = Block::bordered().inner(releases_area);
        app.mouse_hit_map.chat_list = Some(inner);
        app.mouse_hit_map.news_rows = (state.offset()..app.news.note_count())
            .take(inner.height as usize)
            .enumerate()
            .map(|(row, index)| {
                (
                    Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                    index,
                )
            })
            .collect();
    }
    let mut status = vec![
        app.news_update.controls(),
        "j/k or arrows: releases · Ctrl+U/D: scroll · r: refresh".to_string(),
    ];
    if app.news.loading {
        status.push("Loading stable release history…".into());
    }
    if let Some(history) = &app.news.history {
        if history.stale {
            status.push(if history.releases.is_empty() {
                "Offline — no cached newer release notes available".into()
            } else {
                "Offline — cached notes, not a current release check".into()
            });
        }
        if let Some(error) = &history.error {
            status.push(format!("Error: {error}"));
        }
        if let Some(error) = &history.cache_error {
            status.push(format!("Cache error: {error}"));
        }
        if history.incomplete {
            status.push("Incomplete history — more releases may exist".into());
        }
        if !history.stale
            && !history.incomplete
            && history.error.is_none()
            && history.cache_error.is_none()
            && history.releases.is_empty()
        {
            status.push("No newer stable releases found.".into());
        }
    } else if !app.news.loading {
        status.push("History not loaded — press r to refresh".into());
    }
    let status_lines = wrapped_lines(&status.join("\n"), body_area.width);
    let status_height = status_lines
        .len()
        .min(body_area.height.saturating_sub(3) as usize) as u16;
    let status_widget = Paragraph::new(status_lines);
    let [status_area, note_area] =
        Layout::vertical([Constraint::Length(status_height), Constraint::Min(0)]).areas(body_area);
    frame.render_widget(
        status_widget.style(Style::default().fg(NEWS_COLOR)),
        status_area,
    );
    let note = app.news.selected_note();
    let text = format!(
        "{}\n{}\n\n{}",
        note.title,
        note.published_at
            .as_deref()
            .unwrap_or("Publication date unavailable"),
        note.body
    );
    let title = format!("News wptui · {}", note.version);
    let inner = Block::bordered().inner(note_area);
    let lines = wrapped_lines(&text, inner.width);
    let max_scroll = lines.len().saturating_sub(inner.height as usize);
    app.news.scroll = app.news.scroll.min(max_scroll);
    let visible = lines
        .into_iter()
        .skip(app.news.scroll)
        .take(inner.height as usize)
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(visible).block(Block::bordered().title(title).border_style(
            Style::default().fg(if app.focus_pane == FocusPane::Conversation {
                Color::Green
            } else {
                NEWS_COLOR
            }),
        )),
        note_area,
    );
    app.mouse_hit_map.news_body = Some(inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{actions::Section, test_support::TestApp};
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn rail_label_reserves_selector_and_selected_news_has_distinct_legible_style() {
        for (width, label) in [(11, "News"), (12, "News wptui")] {
            assert_eq!(rail_title(width), label);
        }
        let mut app = TestApp::new();
        app.selected_section = Section::News;
        let mut terminal = Terminal::new(TestBackend::new(14, 9)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                super::super::navigation::render_section_rail(frame, &mut app, area);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("> News wptui"));
        assert!(text.contains("Logout"));
        assert!(
            buffer.content.iter().any(|cell| cell.symbol() == "N"
                && cell.fg == Color::Black
                && cell.bg == NEWS_COLOR)
        );
    }
}
