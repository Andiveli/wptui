use std::sync::Arc;

use chrono::{DateTime, Datelike, Local};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Paragraph, Widget},
};
use whatsrust as wr;

use crate::app::App;

#[path = "message_formatting.rs"]
mod message_formatting;
#[path = "message_helpers.rs"]
mod message_helpers;
#[path = "message_layout.rs"]
mod message_layout;
#[path = "message_layout_integration.rs"]
mod message_layout_integration;
#[path = "message_list_reconciliation.rs"]
mod message_list_reconciliation;
#[path = "message_list_state.rs"]
mod message_list_state;
#[path = "message_media.rs"]
mod message_media;
#[path = "message_viewport.rs"]
mod message_viewport;

pub use message_formatting::reaction_chips;
use message_formatting::{
    author_color, media_paragraph, message_block, message_content_area, reaction_line,
};
pub use message_helpers::{
    AUTHOR_GROUP_MAX_GAP, AuthorGroupContext, get_quoted_text, reply_summary, starts_author_group,
};
use message_helpers::{
    StatusLabel, directionally_ordered_spans, fit_text_to_width, forwarding_indicator_lines,
    forwarding_label, inline_content_lines, reply_summary_for_width,
};
pub use message_layout::{
    IMAGE_HEIGHT, IMAGE_WIDTH, MESSAGE_HEIGHT_CACHE_CAPACITY, MessageHeightCache, VIDEO_HEIGHT,
    VIDEO_WIDTH,
};
pub use message_list_state::MessageListState;
pub use message_media::preview_height;
use message_media::render_file;

#[derive(Default)]
pub struct MessageSequenceCache {
    pub ids: Option<Arc<[wr::MessageId]>>,
    pub author_groups: Option<Arc<[AuthorGroupContext]>>,
    built_revision: u64,
    #[cfg(test)]
    operation_counts: SequenceCacheOperationCounts,
}

#[cfg(test)]
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
struct SequenceCacheOperationCounts {
    source_id_iterations: u64,
    message_lookups: u64,
    group_builds: u64,
    signature_allocations: u64,
}

impl MessageSequenceCache {
    pub(crate) fn invalidate(&mut self) {
        self.ids = None;
        self.author_groups = None;
    }

    pub(crate) fn is_valid_for(&self, revision: u64) -> bool {
        self.ids.is_some() && self.author_groups.is_some() && self.built_revision == revision
    }
}

pub(crate) fn chat_message_sequence(
    app: &mut App,
    chat: &wr::JID,
) -> Option<(Arc<[wr::MessageId]>, Arc<[AuthorGroupContext]>, bool)> {
    let revision = app
        .message_sequence_revisions
        .get(chat)
        .copied()
        .unwrap_or(0);
    let cached = app
        .message_sequence_cache
        .get(chat)
        .filter(|cache| cache.is_valid_for(revision))
        .and_then(|cache| cache.ids.as_ref().zip(cache.author_groups.as_ref()))
        .map(|(ids, groups)| (ids.clone(), groups.clone()));
    if let Some((ids, groups)) = cached {
        app.runtime_diagnostics.record_message_sequence_cache_hit();
        return Some((ids, groups, false));
    }

    let started = app.message_sequence_started();
    let source_ids = app.chat_messages.get(chat)?;
    #[cfg(test)]
    let mut operation_counts = SequenceCacheOperationCounts::default();
    let ids = source_ids
        .iter()
        .rev()
        .filter(|id| {
            #[cfg(test)]
            {
                operation_counts.source_id_iterations += 1;
                operation_counts.message_lookups += 1;
            }
            app.messages.contains_key(*id)
        })
        .cloned()
        .collect::<Vec<_>>();
    let groups = ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            #[cfg(test)]
            {
                operation_counts.group_builds += 1;
                operation_counts.message_lookups += 1;
            }
            let message = &app.messages[id];
            let previous = ids
                .get(index + 1)
                .and_then(|previous| app.messages.get(previous));
            AuthorGroupContext::new(starts_author_group(previous, message))
        })
        .collect::<Vec<_>>();
    let ids: Arc<[wr::MessageId]> = ids.into();
    let groups: Arc<[AuthorGroupContext]> = groups.into();
    let cache = app.message_sequence_cache.entry(chat.clone()).or_default();
    cache.ids = Some(ids.clone());
    cache.author_groups = Some(groups.clone());
    cache.built_revision = revision;
    #[cfg(test)]
    {
        cache.operation_counts = operation_counts;
    }
    app.record_message_sequence_finished(started);
    Some((ids, groups, true))
}

#[cfg(test)]
mod sequence_cache_tests {
    use super::*;
    use crate::app::test_support::TestApp;

    #[test]
    fn stable_five_thousand_message_hit_does_no_source_or_message_work() {
        let mut test_app = TestApp::new();
        let chat: wr::JID = "cache-test@example.test".to_owned().into();
        let mut ids = Vec::with_capacity(5_000);
        for index in 0..5_000 {
            let id: wr::MessageId = format!("message-{index}").into();
            ids.push(id.clone());
            test_app.app.messages.insert(
                id.clone(),
                wr::Message {
                    info: wr::MessageInfo {
                        id,
                        chat: chat.clone(),
                        sender: "sender@example.test".to_owned().into(),
                        mentions_self: false,
                        timestamp: index,
                        is_from_me: false,
                        quote_id: None,
                        read_by: 0,
                        forwarding: Default::default(),
                    },
                    message: wr::MessageContent::Text("body".into()),
                },
            );
        }
        test_app.app.chat_messages.insert(chat.clone(), ids);

        let (_, _, rebuilt) = chat_message_sequence(&mut test_app.app, &chat).unwrap();
        assert!(rebuilt);
        let before = test_app
            .app
            .message_sequence_cache
            .get(&chat)
            .unwrap()
            .operation_counts;
        let (_, _, rebuilt) = chat_message_sequence(&mut test_app.app, &chat).unwrap();
        assert!(!rebuilt);
        let after = test_app
            .app
            .message_sequence_cache
            .get(&chat)
            .unwrap()
            .operation_counts;
        assert_eq!(before, after);
        assert_eq!(after.signature_allocations, 0);
        assert_eq!(after.source_id_iterations, 5_000);
        assert_eq!(after.group_builds, 5_000);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MessageTextMode {
    Chat,
    Status,
}

fn status_label(app: &App, message_id: &wr::MessageId) -> Option<StatusLabel> {
    let status = app.message_status(message_id);
    status
        .deleted
        .then_some(StatusLabel::Deleted)
        .or_else(|| status.edited.then_some(StatusLabel::Edited))
}

#[cfg(test)]
mod layout_contract_tests {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Modifier};
    use whatsrust as wr;

    use super::MessageTextMode;
    use super::message_list_state::ViewportAnchor;

    #[test]
    fn rendered_cells_receive_visual_text_once() {
        use ratatui::{
            buffer::Buffer,
            layout::Rect,
            widgets::{Paragraph, Widget},
        };

        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 1));
        Paragraph::new(super::inline_content_lines("abc אבג 123", &[], None, 20))
            .render(Rect::new(0, 0, 20, 1), &mut buffer);
        let rendered = (0..12).map(|x| buffer[(x, 0)].symbol()).collect::<String>();

        assert_eq!(rendered, "abc 123 גבא ");
    }

    #[test]
    fn ratatui_cells_right_align_rtl_and_left_align_ltr_lines() {
        use ratatui::{
            buffer::Buffer,
            layout::Rect,
            widgets::{Paragraph, Widget},
        };

        let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 2));
        Paragraph::new(super::inline_content_lines("אבג\nabc", &[], None, 10))
            .render(Rect::new(0, 0, 10, 2), &mut buffer);

        let rtl = (0..10).map(|x| buffer[(x, 0)].symbol()).collect::<String>();
        let ltr = (0..10).map(|x| buffer[(x, 1)].symbol()).collect::<String>();
        assert_eq!(rtl, "       גבא");
        assert_eq!(ltr, "abc       ");
    }

    #[test]
    fn status_content_keeps_logical_order() {
        let lines =
            super::inline_content_lines("abc אבג 123", &[], Some(super::StatusLabel::Edited), 20);
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(rendered, ["abc 123 גבא (edited)"]);
        assert!(lines[0].spans.iter().any(|span| {
            span.content.as_ref() == "(edited)"
                && span.style == ratatui::style::Style::default().dark_gray()
        }));
    }

    #[test]
    fn rtl_status_label_stays_styled_after_reordering() {
        let lines = super::inline_content_lines("אבג", &[], Some(super::StatusLabel::Edited), 20);
        let line = &lines[0];

        assert_eq!(
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
            ")edited( גבא"
        );
        assert!(line.spans.iter().any(|span| {
            span.content.as_ref() == ")edited("
                && span.style == ratatui::style::Style::default().dark_gray()
        }));
    }

    #[test]
    fn caption_cells_follow_visual_order_after_logical_wrapping() {
        use ratatui::{
            buffer::Buffer,
            layout::Rect,
            widgets::{Paragraph, Widget},
        };

        let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 3));
        Paragraph::new(super::inline_content_lines("abc אבג 123", &[], None, 4))
            .render(Rect::new(0, 0, 4, 3), &mut buffer);
        let rendered = (0..3)
            .map(|y| (0..4).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>();

        assert_eq!(rendered, ["abc ", "גבא ", "123 "]);
    }

    #[test]
    fn text_height_contract_handles_unicode_and_narrow_widths() {
        assert_eq!(super::message_layout::text_height("café 👩‍💻", 20), 1);
        assert_eq!(super::message_layout::text_height("café 👩‍💻", 4), 2);
    }

    #[test]
    fn pending_backend_cells_are_dimmed_for_quote_and_semantic_mention() {
        let mut test_app = crate::app::test_support::TestApp::new();
        let chat: wr::JID = "chat@g.us".to_owned().into();
        let quote_id: wr::MessageId = "quote".into();
        test_app.messages.insert(
            quote_id.clone(),
            wr::Message {
                info: wr::MessageInfo {
                    id: quote_id.clone(),
                    chat: chat.clone(),
                    sender: chat.clone(),
                    mentions_self: false,
                    timestamp: 1,
                    is_from_me: false,
                    quote_id: None,
                    read_by: 0,
                    forwarding: Default::default(),
                },
                message: wr::MessageContent::Text("quoted".into()),
            },
        );
        let pending_id: wr::MessageId = "local-send-1".into();
        wr::store_message_mention_ranges(&pending_id, "@111", vec![0..4]);
        let pending = wr::Message {
            info: wr::MessageInfo {
                id: pending_id,
                chat: chat.clone(),
                sender: chat,
                mentions_self: false,
                timestamp: 2,
                is_from_me: true,
                quote_id: Some(quote_id),
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text("@111".into()),
        };
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                super::render_message(
                    frame.buffer_mut(),
                    &pending,
                    false,
                    super::AuthorGroupContext::STARTS_GROUP,
                    &mut test_app,
                    Rect::new(0, 0, 40, 8),
                    false,
                    MessageTextMode::Chat,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let mut rendered = buffer.content.iter().filter(|cell| {
            let symbol = cell.symbol();
            symbol.contains('@') || symbol.contains('1') || symbol.contains('q')
        });
        assert!(rendered.clone().next().is_some());
        assert!(rendered.all(|cell| cell.modifier.contains(Modifier::DIM)));
    }

    #[test]
    fn pending_tail_overflow_keeps_newest_fit_in_local_order() {
        let mut test_app = crate::app::test_support::TestApp::new();
        test_app.message_list_state.select(Some(7));
        test_app.message_list_state.offset = 3;
        test_app.message_list_state.selected_message = Some("canonical".into());
        test_app.message_list_state.viewport_anchor = Some(ViewportAnchor {
            index: 2,
            y: 4,
            width: 24,
            offset: 3,
            generation: 9,
            message_id: "canonical".into(),
            bottom: 8,
        });
        let state_before = test_app.message_list_state.clone();
        let chat: wr::JID = "chat@g.us".to_owned().into();
        test_app
            .chat_messages
            .insert(chat.clone(), vec!["canonical".into()]);
        let items = ["oldest-pending", "middle-pending", "newest-pending"]
            .into_iter()
            .enumerate()
            .map(|(index, text)| wr::Message {
                info: wr::MessageInfo {
                    id: format!("local-send-{}", index + 1).into(),
                    chat: chat.clone(),
                    sender: chat.clone(),
                    mentions_self: false,
                    timestamp: (index + 1) as i64,
                    is_from_me: true,
                    quote_id: None,
                    read_by: 0,
                    forwarding: Default::default(),
                },
                message: wr::MessageContent::Text(text.into()),
            })
            .collect::<Vec<_>>();
        let backend = TestBackend::new(24, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.buffer_mut().set_string(
                    0,
                    0,
                    "canonical-row",
                    ratatui::style::Style::default(),
                );
                assert_eq!(
                    super::render_pending_tail(
                        frame,
                        &mut test_app,
                        Rect::new(0, 3, 24, 5),
                        &items,
                    ),
                    2
                );
            })
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!rendered.contains("oldest-pending"));
        let middle = rendered
            .find("middle-pending")
            .expect("middle pending suffix row");
        let newest = rendered
            .find("newest-pending")
            .expect("newest pending suffix row");
        assert!(middle < newest, "pending suffix order was not local order");
        assert!(rendered.contains("canonical-row"));
        assert_eq!(test_app.message_list_state, state_before);
        assert_eq!(test_app.chat_messages[&chat], vec!["canonical".into()]);

        let narrow = TestBackend::new(12, 3);
        let mut narrow_terminal = Terminal::new(narrow).unwrap();
        narrow_terminal
            .draw(|frame| {
                super::render_pending_tail(frame, &mut test_app, Rect::new(0, 0, 12, 3), &items);
            })
            .unwrap();
        assert_eq!(test_app.message_list_state, state_before);
    }

    #[test]
    fn logical_wrap_precedes_per_line_visual_reordering() {
        let lines = super::inline_content_lines("abc אבג 123", &[], None, 4);
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(rendered, ["abc", "גבא", "123"]);
    }

    #[test]
    fn visual_wrapping_and_cached_height_agree_at_narrow_width() {
        use std::sync::Arc;

        let body = "abc אבג 123";
        let width = 4;
        let lines = super::inline_content_lines(body, &[], None, width);
        let input = super::message_layout::LayoutInput {
            width,
            is_selected: false,
            has_quote: false,
            has_reactions: false,
            author_group: super::AuthorGroupContext::STARTS_GROUP,
            content: super::message_layout::HeightContent::Text {
                body,
                forwarding: None,
                status: None,
            },
        };
        let mut cache = super::MessageHeightCache::default();
        let cached = super::message_layout::height(&mut cache, &Arc::from("rtl"), &input);
        let caption_input = super::message_layout::LayoutInput {
            content: super::message_layout::HeightContent::File {
                kind: super::message_layout::HeightFileKind::Document,
                caption: Some(body),
                preview_loaded: false,
                forwarding: None,
                status: None,
            },
            ..input.clone()
        };
        let caption_cached =
            super::message_layout::height(&mut cache, &Arc::from("rtl-caption"), &caption_input);

        assert_eq!(cached, lines.len() + 1);
        assert_eq!(caption_cached, lines.len() + 2);
        assert_eq!(super::message_layout::text_height(body, width), lines.len());
    }
}

pub use message_layout_integration::message_height;

fn spacing_after_message(
    index: usize,
    author_groups: &[AuthorGroupContext],
    selected: Option<usize>,
) -> usize {
    let has_older_neighbor = index + 1 < author_groups.len();
    let continuation = !author_groups[index].starts_group();
    let selected_at_boundary = selected == Some(index) || selected == Some(index + 1);

    usize::from(has_older_neighbor && (!continuation || selected_at_boundary))
}

/// When `render_image` is false (partial path and image fully off-screen), show a placeholder
/// instead of StatefulImage so we don't mark the protocol as "transmitted" until we actually
/// send at least one row to the frame.
fn render_message(
    buf: &mut Buffer,
    message: &wr::Message,
    is_selected: bool,
    author_group: AuthorGroupContext,
    app: &mut App,
    area: Rect,
    render_image: bool,
    text_mode: MessageTextMode,
) {
    let is_pending = App::is_pending_message_id(&message.info.id);
    let alignment = match &message.message {
        wr::MessageContent::Text(text) => crate::ui::bidi::Direction::from_text(text).alignment(),
        wr::MessageContent::File(file) => crate::ui::bidi::Direction::from_text(
            file.caption.as_deref().unwrap_or(file.path.as_ref()),
        )
        .alignment(),
    };
    // let alignment = if message.info.is_from_me {
    //     ratatui::layout::Alignment::Right
    // } else {
    //     ratatui::layout::Alignment::Left
    // };

    let timestamp_text = {
        let local_time: DateTime<Local> = DateTime::from_timestamp(message.info.timestamp, 0)
            .unwrap()
            .into();
        if local_time.date_naive() == Local::now().date_naive() {
            local_time.format("%H:%M").to_string()
        } else if local_time.date_naive() == (Local::now() - chrono::Duration::days(1)).date_naive()
        {
            local_time.format("Yesterday %H:%M").to_string()
        } else if local_time > (Local::now() - chrono::Duration::days(7)) {
            local_time.format("%a %H:%M").to_string()
        } else if local_time.year() == Local::now().year() {
            local_time.format("%d %b %H:%M").to_string()
        } else {
            local_time.format("%Y %d %b %H:%M").to_string()
        }
    };
    let timestamp = timestamp_text.clone().italic();

    let sender_name = app.message_sender_name(message);
    let sender_color = author_color(&message.info.sender);

    let mut header = directionally_ordered_spans(
        sender_name.as_ref(),
        Style::default().fg(sender_color).bold(),
    );
    header.extend([" (".into(), timestamp.clone(), ")".into()]);
    if message.info.read_by >= 1 {
        header.push(" ✓".into());
    }
    header.push(" ".into());
    let mut msg_block = message_block(header, timestamp, is_selected, author_group);
    if is_pending {
        msg_block = msg_block.dim();
    }

    // let sender_widget = Line::from_iter(header).alignment(alignment).bold();

    let status = status_label(app, &message.info.id);
    let forwarding = forwarding_label(message);

    let msg_area = message_content_area(msg_block.inner(area), is_selected);
    let quote_lines = message.info.quote_id.as_ref().map(|quote_id| {
        let summary = app
            .messages
            .get(quote_id)
            .map(|quote| {
                reply_summary_for_width(
                    quote,
                    &app.message_sender_name(quote),
                    msg_area.width as usize,
                )
            })
            .unwrap_or_else(|| fit_text_to_width("not found", msg_area.width as usize));
        inline_content_lines(&summary, &[], None, msg_area.width as usize)
            .into_iter()
            .collect::<Vec<_>>()
    });
    // let msg_area = area;
    // let msg_area = if message.info.is_from_me {
    //     let [_, b] = Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)])
    //         .areas(area);
    //     b
    // } else {
    //     let [a, _] = Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)])
    //         .areas(area);
    //     a
    // };

    // let [sender_area, quoted_area, content_area] = Layout::vertical([
    //     Constraint::Length(1),
    //     Quote height is allocated by the active layout below.
    //     Constraint::Min(1),
    // ])
    let [quoted_area, forwarding_area, content_area, reactions_area] = Layout::vertical([
        Constraint::Length(quote_lines.as_ref().map_or(0, Vec::len) as u16),
        Constraint::Length(forwarding_indicator_lines(forwarding, msg_area.width as usize) as u16),
        Constraint::Min(0),
        Constraint::Length(u16::from(app.reactions.contains_key(&message.info.id))),
    ])
    .areas(msg_area);

    msg_block.render(area, buf);
    // sender_widget.render(sender_area, buf);
    if let Some(quote_lines) = quote_lines {
        Paragraph::new(quote_lines)
            .style(Style::default().dark_gray().dim())
            .render(quoted_area, buf);
    }
    if let Some(forwarding) = forwarding {
        Paragraph::new(inline_content_lines(
            forwarding.text(),
            &[],
            None,
            forwarding_area.width as usize,
        ))
        .style(if is_pending {
            Style::default().dark_gray().dim()
        } else {
            Style::default().dark_gray()
        })
        .render(forwarding_area, buf);
    }
    match &message.message {
        wr::MessageContent::Text(text) => {
            let mention_ranges = wr::message_mention_ranges(&message.info.id, text);
            let lines =
                inline_content_lines(text, &mention_ranges, status, content_area.width as usize);
            Paragraph::new(lines)
                .style(if is_pending {
                    Style::default().dim()
                } else {
                    Style::default()
                })
                .alignment(alignment)
                .render(content_area, buf);
        }
        wr::MessageContent::File(data) => {
            render_file(
                buf,
                &message.info.id,
                data,
                status,
                app,
                content_area,
                render_image,
                alignment,
                text_mode,
            );
        }
    };
    if !reactions_area.is_empty() {
        Paragraph::new(reaction_line(
            app.reactions.get(&message.info.id),
            alignment,
        ))
        .render(reactions_area, buf);
    }
}

pub fn render_messages(frame: &mut Frame, app: &mut App, area: Rect) -> Option<()> {
    crate::crash_diagnostics::breadcrumb("first-message-render", "start");
    let chat_jid = app.open_chat()?;
    if area.is_empty() {
        return Some(());
    }
    let assembly_started = app.message_list_phase_started();

    let unread_count = app.unread_boundary(&chat_jid).map_or(0, |(count, _)| count);
    let banner = app.unread_boundary(&chat_jid).map(|(count, since)| {
        format!(
            " {count} unread messages since {}",
            DateTime::<chrono::Utc>::from_timestamp(since, 0)
                .map(DateTime::<Local>::from)
                .unwrap()
                .format("%Y-%m-%d %H:%M")
        )
    });
    let pending = app.pending_new_messages(&chat_jid);
    let optimistic_items = app.pending_messages_for_chat(&chat_jid);
    let notice_height = u16::from(banner.is_some());
    let pending_height = u16::from(pending > 0);
    let optimistic_height = pending_tail_height(app, &optimistic_items, area.width as usize);
    let notice_area = Rect::new(area.x, area.y, area.width, notice_height);
    let list_area = Rect::new(
        area.x,
        area.y.saturating_add(notice_height),
        area.width,
        area.height
            .saturating_sub(notice_height)
            .saturating_sub(pending_height)
            .saturating_sub(optimistic_height),
    );
    let pending_area = Rect::new(
        area.x,
        area.bottom()
            .saturating_sub(optimistic_height)
            .saturating_sub(pending_height),
        area.width,
        pending_height,
    );
    let optimistic_area = Rect::new(
        area.x,
        area.bottom().saturating_sub(optimistic_height),
        area.width,
        optimistic_height,
    );

    let (items, author_groups, sequence_rebuilt) = chat_message_sequence(app, &chat_jid)?;
    let author_groups_built = u64::from(sequence_rebuilt) * items.len() as u64;
    app.record_message_list_counts(crate::app::runtime_diagnostics::MessageListCounts {
        canonical_messages_cloned: 0,
        pending_candidates: optimistic_items.len() as u64,
        ..Default::default()
    });
    app.finish_message_list_phase(
        crate::app::runtime_diagnostics::Phase::MessageAssembly,
        assembly_started,
    );
    let result = render_message_items(
        frame,
        app,
        list_area,
        items,
        author_groups,
        sequence_rebuilt,
        author_groups_built,
        unread_count,
        MessageTextMode::Chat,
    );
    let pending_started = app.message_list_phase_started();
    let pending_rows_rendered = render_pending_tail(frame, app, optimistic_area, &optimistic_items);
    app.record_message_list_counts(crate::app::runtime_diagnostics::MessageListCounts {
        pending_rows_rendered: pending_rows_rendered as u64,
        ..Default::default()
    });
    app.finish_message_list_phase(
        crate::app::runtime_diagnostics::Phase::MessagePendingTail,
        pending_started,
    );
    crate::crash_diagnostics::breadcrumb("first-message-render", "complete");
    let overlays_started = app.message_list_phase_started();
    if let Some(text) = banner {
        Paragraph::new(text)
            .style(
                Style::default()
                    .fg(Color::Reset)
                    .bg(Color::Rgb(37, 211, 102)),
            )
            .render(notice_area, frame.buffer_mut());
    }
    if pending > 0 && !pending_area.is_empty() {
        let label = format!("↓ {pending} new messages ");
        let width = label.chars().count().min(pending_area.width as usize) as u16;
        let x = pending_area.x + pending_area.width.saturating_sub(width) / 2;
        Paragraph::new(Line::styled(label, Style::default().fg(Color::Cyan).bold()))
            .render(Rect::new(x, pending_area.y, width, 1), frame.buffer_mut());
    }
    app.finish_message_list_phase(
        crate::app::runtime_diagnostics::Phase::MessageOverlays,
        overlays_started,
    );
    result
}

fn pending_tail_height(app: &mut App, items: &[wr::Message], width: usize) -> u16 {
    let groups = items
        .iter()
        .enumerate()
        .map(|(index, message)| {
            AuthorGroupContext::new(starts_author_group(items.get(index + 1), message))
        })
        .collect::<Vec<_>>();
    items
        .iter()
        .enumerate()
        .map(|(index, message)| {
            message_height(message, width, false, groups[index], app)
                + spacing_after_message(index, &groups, None)
        })
        .sum::<usize>()
        .min(u16::MAX as usize) as u16
}

fn render_pending_tail(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    items: &[wr::Message],
) -> usize {
    if area.is_empty() || items.is_empty() {
        return 0;
    }
    let groups = items
        .iter()
        .enumerate()
        .map(|(index, message)| {
            AuthorGroupContext::new(starts_author_group(items.get(index + 1), message))
        })
        .collect::<Vec<_>>();
    let heights = items
        .iter()
        .enumerate()
        .map(|(index, message)| {
            message_height(message, area.width as usize, false, groups[index], app)
                + spacing_after_message(index, &groups, None)
        })
        .collect::<Vec<_>>();
    let newest = items.len() - 1;
    let mut first = newest;
    let mut used = heights[newest].min(area.height as usize);
    for index in (0..newest).rev() {
        let candidate = used.saturating_add(heights[index]);
        if candidate > area.height as usize {
            break;
        }
        first = index;
        used = candidate;
    }
    let mut y = area.bottom().saturating_sub(used as u16);
    let mut rendered_rows = 0;
    for (index, message) in items.iter().enumerate().skip(first) {
        let height =
            heights[index].saturating_sub(spacing_after_message(index, &groups, None)) as u16;
        if height == 0 || y >= area.bottom() {
            continue;
        }
        let render_height = height.min(area.bottom().saturating_sub(y));
        render_message(
            frame.buffer_mut(),
            message,
            false,
            groups[index],
            app,
            Rect::new(area.x, y, area.width, render_height),
            false,
            MessageTextMode::Chat,
        );
        rendered_rows += 1;
        y = y.saturating_add(heights[index] as u16);
    }
    rendered_rows
}

/// Read-only statuses of the opened status contact, rendered with the
/// same machinery as chats (media previews, timestamps, sender header).
pub fn render_status_messages(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(contact) = app.open_status_contact() else {
        return;
    };
    let items: Arc<[wr::MessageId]> = app
        .status_messages(&contact)
        .iter()
        .rev()
        .filter(|id| app.messages.contains_key(*id))
        .cloned()
        .collect::<Vec<_>>()
        .into();
    let author_groups: Arc<[AuthorGroupContext]> = items
        .iter()
        .enumerate()
        .map(|(index, id)| {
            AuthorGroupContext::new(starts_author_group(
                items.get(index + 1).and_then(|id| app.messages.get(id)),
                app.messages.get(id).expect("status message must exist"),
            ))
        })
        .collect::<Vec<_>>()
        .into();
    render_message_items(
        frame,
        app,
        area,
        items.clone(),
        author_groups,
        true,
        items.len() as u64,
        0,
        MessageTextMode::Status,
    );
}

/// Shared core of chat and status message rendering. The caller supplies
/// the messages newest-first (as rendered top-to-bottom).
fn render_message_items(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    items: Arc<[wr::MessageId]>,
    author_groups: Arc<[AuthorGroupContext]>,
    sequence_rebuilt: bool,
    author_groups_built: u64,
    unread_count: usize,
    text_mode: MessageTextMode,
) -> Option<()> {
    let list_area = area;
    if list_area.is_empty() {
        return Some(());
    }

    let preparation_started = app.message_list_phase_started();
    let measurements_before = app.message_height_cache.measurement_count();
    app.record_message_list_counts(crate::app::runtime_diagnostics::MessageListCounts {
        author_groups_built,
        ..Default::default()
    });

    if sequence_rebuilt {
        message_layout_integration::retain_message_heights(app, &items);
        app.record_message_list_counts(crate::app::runtime_diagnostics::MessageListCounts {
            height_cache_retained_count: app.message_height_cache.len() as u64,
            ..Default::default()
        });
    }
    app.finish_message_list_phase(
        crate::app::runtime_diagnostics::Phase::MessagePreparation,
        preparation_started,
    );

    if items.is_empty() {
        app.message_list_state.select(None);
        return Some(());
    }

    let width = list_area.width as isize;
    let selection_started = app.message_list_phase_started();
    let (start_index, y) = message_list_reconciliation::reconcile(
        app,
        list_area,
        &items,
        &author_groups,
        unread_count,
    );
    app.finish_message_list_phase(
        crate::app::runtime_diagnostics::Phase::MessageSelectionReconciliation,
        selection_started,
    );
    let viewport_started = app.message_list_phase_started();
    let mut viewport_counts = crate::app::runtime_diagnostics::MessageListCounts::default();
    app.message_list_state.viewport_anchor = message_viewport::render(
        frame,
        app,
        list_area,
        &items,
        &author_groups,
        unread_count,
        width,
        start_index,
        y,
        &mut viewport_counts,
        text_mode,
    );
    app.record_message_list_counts(crate::app::runtime_diagnostics::MessageListCounts {
        visible_rows: viewport_counts.visible_rows,
        temporary_buffer_rows: viewport_counts.temporary_buffer_rows,
        media_rows: viewport_counts.media_rows,
        receipt_candidates: viewport_counts.receipt_candidates,
        height_measurements: app
            .message_height_cache
            .measurement_count()
            .saturating_sub(measurements_before),
        ..Default::default()
    });
    app.finish_message_list_phase(
        crate::app::runtime_diagnostics::Phase::MessageViewportTotal,
        viewport_started,
    );

    None
}
