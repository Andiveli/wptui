use crate::app::App;
use crate::app::actions::{FocusPane, Section};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, List, ListItem, Paragraph},
};
use tui_logger::TuiLoggerWidget;

pub(super) fn render_section_rail(frame: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn render_structural_placeholder(frame: &mut Frame, app: &App, area: Rect) {
    let section = app.selected_section.title();
    frame.render_widget(
        Paragraph::new(format!("{section} is not available yet."))
            .block(Block::bordered().title(section)),
        area,
    );
}

pub(super) fn render_logout_placeholder(frame: &mut Frame, app: &App, area: Rect) {
    let content = if app.logout_in_progress {
        "Logging out…\n\nThis removes the device from WhatsApp and clears the local session."
            .to_string()
    } else if app.pending_logout {
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

pub(super) fn render_logs(frame: &mut Frame, area: Rect) {
    let log_widget = TuiLoggerWidget::default()
        .style_trace(Style::new().dark_gray())
        .style_debug(Style::new().blue())
        .style_warn(Style::new().yellow())
        .style_error(Style::new().red().bold())
        .block(
            Block::default()
                .title("Logs")
                .borders(ratatui::widgets::Borders::ALL),
        );
    frame.render_widget(log_widget, area);
}
