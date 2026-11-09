use std::sync::Arc;

use chrono::{DateTime, Datelike, Local};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Paragraph, Widget},
};
use whatsrust as wr;

use crate::app::App;
use crate::app::actions::{ConversationMode, FocusPane};
use crate::app::events::MediaRenderPlan;
use crate::app::read_receipts::VisibilityPlan;

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
#[path = "message_preview_cache.rs"]
mod message_preview_cache;
#[cfg(test)]
#[path = "message_preview_tests.rs"]
mod message_preview_tests;
#[path = "message_stacks.rs"]
mod message_stacks;
#[path = "message_viewport.rs"]
mod message_viewport;

pub(crate) use message_preview_cache::bounded_source as bounded_stack_source;
pub use message_preview_cache::{CropResults, StackPreviewCache};

pub use message_formatting::reaction_chips;
use message_formatting::{
    author_color, media_paragraph, message_block, message_content_area, reaction_line,
};
pub use message_helpers::{
    AUTHOR_GROUP_MAX_GAP, AuthorGroupContext, get_quoted_text, reply_summary, starts_author_group,
};
use message_helpers::{
    StatusLabel, directionally_ordered_spans, fit_text_to_width, forwarding_indicator_lines,
    forwarding_label, inline_content_lines, markdown_content_lines, reply_summary_for_width,
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
    use crate::app::events::MediaRenderPlan;

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
        let mut media_render_plan = MediaRenderPlan::default();
        terminal
            .draw(|frame| {
                super::render_message(
                    frame.buffer_mut(),
                    &pending,
                    false,
                    super::AuthorGroupContext::STARTS_GROUP,
                    &mut test_app,
                    Rect::new(0, 0, 40, 8),
                    &mut media_render_plan,
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
    fn view_once_unavailable_is_subdued_selectable_message_card() {
        let mut test_app = crate::app::test_support::TestApp::new();
        let message = wr::Message {
            info: wr::MessageInfo {
                id: "view-once".into(),
                chat: "chat@g.us".to_owned().into(),
                sender: "sender@s.whatsapp.net".to_owned().into(),
                mentions_self: false,
                timestamp: 0,
                is_from_me: false,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::ViewOnceUnavailable,
        };
        let backend = TestBackend::new(60, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut media_render_plan = MediaRenderPlan::default();
        terminal
            .draw(|frame| {
                super::render_message(
                    frame.buffer_mut(),
                    &message,
                    true,
                    super::AuthorGroupContext::STARTS_GROUP,
                    &mut test_app,
                    Rect::new(0, 0, 60, 4),
                    &mut media_render_plan,
                    false,
                    MessageTextMode::Chat,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("View-once media is unavailable here. View it on your"));
        assert!(rendered.contains("phone."));
        assert!(rendered.contains("sender@s.whatsapp.net"));
        assert!(rendered.contains("sender@s.whatsapp.net ("));
        assert!(
            buffer
                .content
                .iter()
                .any(|cell| cell.fg == ratatui::style::Color::Green)
        );
        let description_cell = buffer
            .content
            .iter()
            .find(|cell| cell.symbol() == "V")
            .expect("informational description cell");
        assert!(description_cell.modifier.contains(Modifier::DIM));
        assert_eq!(description_cell.fg, ratatui::style::Color::DarkGray);
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
        let mut media_render_plan = MediaRenderPlan::default();
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
                        &mut media_render_plan,
                        Rect::new(0, 3, 24, 5),
                        &items,
                        super::MessageTextMode::Chat,
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
        let mut narrow_media_render_plan = MediaRenderPlan::default();
        narrow_terminal
            .draw(|frame| {
                super::render_pending_tail(
                    frame,
                    &mut test_app,
                    &mut narrow_media_render_plan,
                    Rect::new(0, 0, 12, 3),
                    &items,
                    super::MessageTextMode::Chat,
                );
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
    fn outgoing_multiline_headings_and_quotes_hide_markers_with_styles_and_keep_source() {
        use ratatui::buffer::Buffer;

        // One message, not independent parser instances: include both kinds of
        // fences and surrounding text to exercise paragraph offsets and code ownership.
        let source = concat!(
            "# Heading one\n## Heading two\n### Heading three\n",
            "#### Heading four\n##### Heading five\n###### Heading six\n\n",
            "*bold* _italic_ ~strike~ `inline` **strong** **_nested_**\n",
            "> Quote one\n> Quote two\n- item\n1. item\n",
            "before plain\n```\n# literal heading\n> literal quote\n```\n",
            "between fences\n```rust\n## rust heading\n> rust quote\n```\n",
            "after café\n"
        );
        let expected = [
            ("Heading one", Modifier::BOLD),
            ("Heading two", Modifier::BOLD),
            ("Heading three", Modifier::BOLD),
            ("Heading four", Modifier::BOLD),
            ("Heading five", Modifier::BOLD),
            ("Heading six", Modifier::BOLD),
            ("", Modifier::empty()),
            ("bold italic strike inline strong nested", Modifier::empty()),
            ("Quote one", Modifier::ITALIC | Modifier::DIM),
            ("Quote two", Modifier::ITALIC | Modifier::DIM),
            ("- item", Modifier::empty()),
            ("1. item", Modifier::empty()),
            ("before plain", Modifier::empty()),
            ("# literal heading", Modifier::REVERSED),
            ("> literal quote", Modifier::REVERSED),
            ("between fences", Modifier::empty()),
            ("## rust heading", Modifier::REVERSED),
            ("> rust quote", Modifier::REVERSED),
            ("after café", Modifier::empty()),
            ("", Modifier::empty()),
        ];
        let mut app = crate::app::test_support::TestApp::new();
        let message = wr::Message {
            info: wr::MessageInfo {
                id: "outgoing-markdown-block".into(),
                chat: "chat@g.us".to_owned().into(),
                sender: "sender@s.whatsapp.net".to_owned().into(),
                mentions_self: false,
                timestamp: 0,
                is_from_me: true,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text(source.into()),
        };
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 24));
        super::render_message(
            &mut buffer,
            &message,
            false,
            super::AuthorGroupContext::CONTINUATION,
            &mut app,
            Rect::new(0, 0, 80, 24),
            &mut MediaRenderPlan::default(),
            false,
            MessageTextMode::Chat,
        );
        for (y, (text, modifier)) in expected.iter().enumerate() {
            let painted = (0..80)
                .map(|x| buffer[(x, y as u16)].symbol())
                .collect::<String>();
            assert_eq!(painted.trim_end(), *text, "outgoing row {y}");
            for x in 0..text.chars().count() {
                assert!(
                    buffer[(x as u16, y as u16)].modifier.contains(*modifier),
                    "style at outgoing row {y}, cell {x}: {text}"
                );
            }
        }
        assert!(matches!(
            &message.message,
            wr::MessageContent::Text(text) if text.as_ref() == source
        ));
    }

    fn url_message(source: &str, outgoing: bool) -> wr::Message {
        wr::Message {
            info: wr::MessageInfo {
                id: "incoming-url-contract".into(),
                chat: "url@example.test".to_owned().into(),
                sender: "sender@example.test".to_owned().into(),
                mentions_self: false,
                timestamp: 0,
                is_from_me: outgoing,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text(source.into()),
        }
    }

    fn render_url_message(
        message: &wr::Message,
        width: u16,
    ) -> (crate::app::test_support::TestApp, ratatui::buffer::Buffer) {
        let mut app = crate::app::test_support::TestApp::new();
        app.add_message(message.clone());
        app.open_chat = Some(message.info.chat.clone());
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        terminal
            .draw(|frame| {
                super::render_messages_with_plan(
                    frame,
                    &mut app,
                    &mut MediaRenderPlan::default(),
                    &mut crate::app::read_receipts::VisibilityPlan::default(),
                    Rect::new(0, 0, width, 24),
                );
            })
            .unwrap();
        assert_eq!(app.messages.len(), 1);
        assert_eq!(
            app.chat_messages[&message.info.chat],
            vec![message.info.id.clone()]
        );
        if let wr::MessageContent::Text(source) = &message.message {
            assert!(matches!(
                &app.messages[&message.info.id].message,
                wr::MessageContent::Text(stored) if stored == source
            ));
        }
        (app, terminal.backend().buffer().clone())
    }

    // The oracle lays out an already specified label, never a Markdown projection.
    // Actual output, hits and height all go through the real incoming-message gate.
    fn assert_url_label(source: &str, label: &str, outgoing: bool, width: u16) {
        use ratatui::{buffer::Buffer, style::Color, widgets::Widget};

        let message = url_message(source, outgoing);
        let (mut app, buffer) = render_url_message(&message, width);
        let lines = super::inline_content_lines(label, &[0..label.len()], None, width as usize);
        let first_text = lines[0].to_string();
        let first = (0..buffer.area.height)
            .find(|&y| screenshot_row(&buffer, y) == first_text.trim_end())
            .unwrap_or_else(|| panic!("missing label {label:?} for {source:?}"));
        let mut expected = Buffer::empty(Rect::new(0, 0, width, lines.len() as u16));
        ratatui::widgets::Paragraph::new(lines.clone()).render(expected.area, &mut expected);
        let mut expected_hits = Vec::new();
        for y in 0..expected.area.height {
            assert_eq!(
                screenshot_row(&buffer, first + y),
                screenshot_row(&expected, y)
            );
            for x in 0..width {
                if expected[(x, y)].fg == Color::Blue {
                    expected_hits.push(Rect::new(x, first + y, 1, 1));
                    if !outgoing && label != source {
                        assert!(
                            buffer[(x, first + y)]
                                .modifier
                                .contains(Modifier::UNDERLINED)
                        );
                    }
                }
            }
        }
        assert!(!expected_hits.is_empty());
        assert_eq!(
            app.mouse_hit_map
                .urls
                .iter()
                .map(|hit| hit.area)
                .collect::<Vec<_>>(),
            expected_hits,
            "hit cells for {source} at width {width}"
        );
        assert!(
            app.mouse_hit_map
                .urls
                .iter()
                .all(|hit| { hit.id == message.info.id && hit.url == source })
        );
        assert_eq!(
            super::message_height(
                &message,
                width as usize,
                false,
                super::AuthorGroupContext::STARTS_GROUP,
                &mut app,
            ),
            lines.len() + 1,
            "public height must measure the displayed label, not the source"
        );
    }

    #[test]
    fn incoming_bare_urls_use_accepted_domain_and_final_segment_labels() {
        for (source, label) in [
            ("https://github.com/Andiveli/wptui", "github.com · wptui"),
            (
                "https://example.com/docs/installation",
                "example.com · installation",
            ),
            ("https://example.com/", "example.com"),
        ] {
            assert_url_label(source, label, false, 80);
        }
    }

    #[test]
    fn incoming_labels_ignore_query_fragment_and_empty_final_segments_only_for_display() {
        for (source, label) in [
            (
                "http://example.com/docs/install///?q=one#part",
                "example.com · install",
            ),
            ("https://example.com?q=one#part", "example.com"),
            ("https://example.com/?q=one#part", "example.com"),
            (
                "https://example.com/docs/%69nstall",
                "example.com · %69nstall",
            ),
        ] {
            assert_url_label(source, label, false, 80);
        }
    }

    #[test]
    fn incoming_label_keeps_complete_subdomain_and_port() {
        assert_url_label(
            "https://docs.api.example.com:8443/a/install",
            "docs.api.example.com:8443 · install",
            false,
            80,
        );
    }

    #[test]
    fn incoming_userinfo_authority_never_masquerades_as_the_userinfo_site() {
        let source = "https://github.com@evil.example/path";
        let message = url_message(source, false);
        let (app, buffer) = render_url_message(&message, 80);
        let rows = (0..buffer.area.height)
            .map(|y| screenshot_row(&buffer, y))
            .collect::<Vec<_>>();
        // Either conservative raw fallback or a correctly parsed actual host is safe.
        assert!(
            rows.iter()
                .any(|row| row == source || row == "evil.example · path")
        );
        assert!(!rows.iter().any(|row| row == "github.com · path"));
        assert!(!app.mouse_hit_map.urls.is_empty());
        assert!(app.mouse_hit_map.urls.iter().all(|hit| hit.url == source));
        let label = if rows.iter().any(|row| row == source) {
            source
        } else {
            "evil.example · path"
        };
        assert_url_label(source, label, false, 80);
    }

    #[test]
    fn incoming_unsupported_authority_remains_raw() {
        let source = "https://example.com:bad/path";
        assert_url_label(source, source, false, 80);
    }

    #[test]
    fn incoming_status_text_keeps_full_url_instead_of_chat_label() {
        let source = "https://example.com/docs/install";
        let message = url_message(source, false);
        let mut app = crate::app::test_support::TestApp::new();
        let mut terminal = Terminal::new(TestBackend::new(80, 2)).unwrap();
        terminal
            .draw(|frame| {
                super::render_message(
                    frame.buffer_mut(),
                    &message,
                    false,
                    super::AuthorGroupContext::CONTINUATION,
                    &mut app,
                    Rect::new(0, 0, 80, 2),
                    &mut MediaRenderPlan::default(),
                    false,
                    MessageTextMode::Status,
                );
            })
            .unwrap();
        assert_eq!(screenshot_row(terminal.backend().buffer(), 0), source);
        assert!(!app.mouse_hit_map.urls.is_empty());
        assert!(app.mouse_hit_map.urls.iter().all(|hit| hit.url == source));
    }

    #[test]
    fn incoming_label_wrap_height_and_mouse_cells_agree_at_narrow_width() {
        for width in [12, 20] {
            assert_url_label(
                "https://example.com/docs/installation?q=one#part",
                "example.com · installation",
                false,
                width,
            );
        }
    }

    #[test]
    fn outgoing_bare_urls_keep_full_source_display_and_mouse_destination() {
        for source in [
            "https://github.com/Andiveli/wptui",
            "https://example.com/docs/installation",
            "https://example.com/",
        ] {
            assert_url_label(source, source, true, 80);
        }
    }

    #[test]
    fn incoming_explicit_labels_code_and_unsupported_literals_are_not_shortened() {
        let url = "https://example.com/docs/install";
        for (source, visible, modifier) in [
            (
                format!("[site]({url})"),
                "site".to_owned(),
                Modifier::UNDERLINED,
            ),
            (
                format!("[{url}]({url})"),
                url.to_owned(),
                Modifier::UNDERLINED,
            ),
            (format!("`{url}`"), url.to_owned(), Modifier::REVERSED),
            (
                format!("```\n{url}\n```"),
                url.to_owned(),
                Modifier::REVERSED,
            ),
            (
                format!("![image]({url})"),
                format!("![image]({url})"),
                Modifier::empty(),
            ),
            (format!("[{url}]"), format!("[{url}]"), Modifier::empty()),
        ] {
            let (app, buffer) = render_url_message(&url_message(&source, false), 80);
            let y = (0..buffer.area.height)
                .find(|&y| screenshot_row(&buffer, y) == visible)
                .unwrap_or_else(|| panic!("literal/label changed: {source}"));
            for x in 0..visible.chars().count() as u16 {
                assert!(buffer[(x, y)].modifier.contains(modifier));
                if modifier.is_empty() {
                    assert_eq!(buffer[(x, y)].modifier, Modifier::empty());
                }
            }
            let start = visible.find(url).unwrap_or(0);
            let length = if visible == "site" {
                visible.len()
            } else {
                url.len()
            };
            assert_eq!(
                app.mouse_hit_map
                    .urls
                    .iter()
                    .map(|hit| hit.area)
                    .collect::<Vec<_>>(),
                (start..start + length)
                    .map(|x| Rect::new(x as u16, y, 1, 1))
                    .collect::<Vec<_>>(),
                "preserved hit cells for {source}"
            );
            assert!(app.mouse_hit_map.urls.iter().all(|hit| hit.url == url));
        }
    }

    #[test]
    fn incoming_file_caption_reply_and_informational_paths_keep_existing_text() {
        let source = "https://example.com/docs/install";
        let mut message = url_message(source, false);
        assert_eq!(
            super::reply_summary(&message, "sender"),
            format!("> sender: {source}")
        );
        message.message = wr::MessageContent::File(wr::FileContent {
            path: "document.txt".into(),
            kind: wr::FileKind::Document,
            caption: Some(source.into()),
            ..Default::default()
        });
        let (app, buffer) = render_url_message(&message, 80);
        assert!((0..buffer.area.height).any(|y| screenshot_row(&buffer, y) == source));
        assert!(matches!(
            &app.messages[&message.info.id].message,
            wr::MessageContent::File(file) if file.caption.as_deref() == Some(source)
        ));
        message.message = wr::MessageContent::ViewOnceUnavailable;
        let (_, buffer) = render_url_message(&message, 80);
        let rendered = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains(wr::VIEW_ONCE_UNAVAILABLE_DESCRIPTION));
    }

    const SCREENSHOT_MARKDOWN: &str = concat!(
        "# Encabezado 1\n## Encabezado 2\n### Encabezado 3\n\n",
        "> Una cita de prueba\n\n",
        "[Ejemplo](https://example.com)\n\n",
        "[enlace sin destino]\n",
        "![imagen no soportada](https://example.com/image.png)"
    );

    // Owner-provided source from L18: only the first heading is at column zero.
    const OWNER_INDENTED_MARKDOWN: &str = concat!(
        "# Encabezado 1\n",
        "   ## Encabezado 2\n",
        "   ### Encabezado 3\n\n",
        "   > Una cita de prueba\n\n",
        "   [Ejemplo](https://example.com)\n\n",
        "   [enlace sin destino]\n",
        "   ![imagen no soportada](https://example.com/image.png)"
    );

    fn render_screenshot_markdown(source: &str) -> ratatui::buffer::Buffer {
        let mut app = crate::app::test_support::TestApp::new();
        let chat: wr::JID = "screenshot@example.test".to_owned().into();
        let id: wr::MessageId = "screenshot-markdown".into();
        app.add_message(wr::Message {
            info: wr::MessageInfo {
                id: id.clone(),
                chat: chat.clone(),
                sender: "sender@example.test".to_owned().into(),
                mentions_self: false,
                timestamp: 0,
                is_from_me: true,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text(source.into()),
        });
        app.open_chat = Some(chat);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                // Exercise the public list path, including height/viewport planning,
                // rather than projecting each syntax example independently.
                super::render_messages_with_plan(
                    frame,
                    &mut app,
                    &mut MediaRenderPlan::default(),
                    &mut crate::app::read_receipts::VisibilityPlan::default(),
                    Rect::new(0, 0, 80, 24),
                );
            })
            .unwrap();
        assert!(matches!(
            &app.messages[&id].message,
            wr::MessageContent::Text(text) if text.as_ref() == source
        ));
        terminal.backend().buffer().clone()
    }

    fn screenshot_row(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    fn assert_screenshot_blocks(source: &str) {
        let buffer = render_screenshot_markdown(source);
        let first = (0..buffer.area.height)
            .find(|&y| screenshot_row(&buffer, y) == "Encabezado 1")
            .expect("first heading must be visible without its hash");
        for (offset, text, modifier) in [
            (0, "Encabezado 1", Modifier::BOLD),
            (1, "Encabezado 2", Modifier::BOLD),
            (2, "Encabezado 3", Modifier::BOLD),
            (3, "", Modifier::empty()),
            (4, "Una cita de prueba", Modifier::ITALIC | Modifier::DIM),
            (5, "", Modifier::empty()),
            (6, "Ejemplo", Modifier::UNDERLINED),
            (7, "", Modifier::empty()),
        ] {
            let y = first + offset;
            assert_eq!(screenshot_row(&buffer, y), text, "row {offset}: {source:?}");
            for x in 0..text.chars().count() as u16 {
                assert!(
                    buffer[(x, y)].modifier.contains(modifier),
                    "row {offset}, cell {x}: {source:?}"
                );
            }
        }
    }

    fn assert_screenshot_literals(source: &str) {
        let buffer = render_screenshot_markdown(source);
        // Anchor at the preceding valid link so a missing opening bracket cannot
        // prevent the malformed-link assertion from identifying the changed row.
        let link_row = (0..buffer.area.height)
            .find(|&y| screenshot_row(&buffer, y) == "Ejemplo")
            .expect("valid link must still show only its label");
        assert_eq!(screenshot_row(&buffer, link_row + 1), "");
        for (offset, text) in [
            (2, "[enlace sin destino]"),
            (3, "![imagen no soportada](https://example.com/image.png)"),
        ] {
            let y = link_row + offset;
            assert_eq!(screenshot_row(&buffer, y), text, "row {offset}: {source:?}");
            for x in 0..text.len() as u16 {
                assert_eq!(
                    buffer[(x, y)].modifier,
                    Modifier::empty(),
                    "literal style at row {offset}, cell {x}: {source:?}"
                );
            }
        }
    }

    #[test]
    fn combined_screenshot_lf_headings_quotes_and_valid_link_are_formatted() {
        assert_screenshot_blocks(SCREENSHOT_MARKDOWN);
    }

    #[test]
    fn combined_screenshot_crlf_headings_quotes_and_valid_link_are_formatted() {
        assert_screenshot_blocks(&SCREENSHOT_MARKDOWN.replace('\n', "\r\n"));
    }

    #[test]
    fn combined_screenshot_lf_malformed_link_and_http_image_stay_literal() {
        assert_screenshot_literals(SCREENSHOT_MARKDOWN);
    }

    #[test]
    fn combined_screenshot_crlf_malformed_link_and_http_image_stay_literal() {
        assert_screenshot_literals(&SCREENSHOT_MARKDOWN.replace('\n', "\r\n"));
    }

    #[test]
    fn owner_three_space_headings_hide_markers_and_render_bold() {
        let buffer = render_screenshot_markdown(OWNER_INDENTED_MARKDOWN);
        let first = (0..buffer.area.height)
            .find(|&y| screenshot_row(&buffer, y) == "Encabezado 1")
            .expect("first heading must be visible without its hash");
        for (offset, text) in [
            (0, "Encabezado 1"),
            (1, "Encabezado 2"),
            (2, "Encabezado 3"),
        ] {
            let y = first + offset;
            assert_eq!(screenshot_row(&buffer, y), text, "heading row {offset}");
            for x in 0..text.len() as u16 {
                assert!(buffer[(x, y)].modifier.contains(Modifier::BOLD));
            }
        }
        assert_eq!(screenshot_row(&buffer, first + 3), "");
    }

    #[test]
    fn owner_three_space_quote_hides_marker_and_renders_italic_dim() {
        let buffer = render_screenshot_markdown(OWNER_INDENTED_MARKDOWN);
        // Anchor at the unchanged first heading, not a heading that is currently
        // broken, so the quote rule produces its own independent failure.
        let first = (0..buffer.area.height)
            .find(|&y| screenshot_row(&buffer, y) == "Encabezado 1")
            .expect("first heading must be visible without its hash");
        let y = first + 4;
        let text = "Una cita de prueba";
        assert_eq!(screenshot_row(&buffer, y), text);
        for x in 0..text.len() as u16 {
            assert!(
                buffer[(x, y)]
                    .modifier
                    .contains(Modifier::ITALIC | Modifier::DIM)
            );
        }
        assert_eq!(screenshot_row(&buffer, y - 1), "");
        assert_eq!(screenshot_row(&buffer, y + 1), "");
    }

    #[test]
    fn owner_three_space_valid_label_and_unsupported_literals_are_preserved() {
        assert_screenshot_literals(OWNER_INDENTED_MARKDOWN);
        let buffer = render_screenshot_markdown(OWNER_INDENTED_MARKDOWN);
        let y = (0..buffer.area.height)
            .find(|&y| screenshot_row(&buffer, y) == "Ejemplo")
            .expect("valid link must show only Ejemplo, without its destination");
        for x in 0.."Ejemplo".len() as u16 {
            assert!(buffer[(x, y)].modifier.contains(Modifier::UNDERLINED));
        }
    }

    #[test]
    fn labeled_link_renders_only_styled_label_and_hits_only_label_cells() {
        use ratatui::buffer::Buffer;

        let source = "before [Ejemplo](https://example.com) after";
        let mut app = crate::app::test_support::TestApp::new();
        let id: wr::MessageId = "labeled-link".into();
        let message = wr::Message {
            info: wr::MessageInfo {
                id: id.clone(),
                chat: "chat@g.us".to_owned().into(),
                sender: "sender@s.whatsapp.net".to_owned().into(),
                mentions_self: false,
                timestamp: 0,
                is_from_me: true,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::Text(source.into()),
        };
        let mut buffer = Buffer::empty(Rect::new(0, 0, 60, 3));
        super::render_message(
            &mut buffer,
            &message,
            false,
            super::AuthorGroupContext::CONTINUATION,
            &mut app,
            Rect::new(3, 1, 50, 1),
            &mut MediaRenderPlan::default(),
            false,
            MessageTextMode::Chat,
        );
        let hits = &app.mouse_hit_map.urls;
        assert_eq!(hits.len(), "Ejemplo".len());
        for (index, hit) in hits.iter().enumerate() {
            assert_eq!(hit.id, id);
            assert_eq!(hit.url, "https://example.com");
            assert_eq!(hit.area, Rect::new(10 + index as u16, 1, 1, 1));
            let cell = &buffer[(hit.area.x, hit.area.y)];
            assert_eq!(cell.symbol(), &"Ejemplo"[index..index + 1]);
            assert!(cell.modifier.contains(Modifier::UNDERLINED));
        }
        let painted = (3..53).map(|x| buffer[(x, 1)].symbol()).collect::<String>();
        assert_eq!(painted.trim_end(), "before Ejemplo after");
        assert!(matches!(
            &message.message,
            wr::MessageContent::Text(text) if text.as_ref() == source
        ));
    }

    #[test]
    fn formatted_bare_url_keeps_its_original_mouse_destination() {
        let body = "https://example.com/_docs_";
        let mut app = crate::app::test_support::TestApp::new();
        super::record_url_hits(
            &mut app,
            &"original-url".into(),
            body,
            None,
            Rect::new(0, 0, 40, 2),
            ratatui::layout::Alignment::Left,
        );
        assert!(!app.mouse_hit_map.urls.is_empty());
        assert!(app.mouse_hit_map.urls.iter().all(|hit| hit.url == body));
        let visible = super::markdown_content_lines(body, &[], None, 40, &[]);
        assert_eq!(visible[0].to_string(), body);
    }

    #[test]
    fn formatted_urls_keep_exact_painted_hit_cells() {
        use ratatui::{
            buffer::Buffer,
            layout::Alignment,
            widgets::{Paragraph, Widget},
        };
        let mut app = crate::app::test_support::TestApp::new();
        for (body, expected_cells) in [
            ("**https://example.com**", "https://example.com"),
            ("[site](https://example.com)", "site"),
            ("`https://example.com`", "https://example.com"),
        ] {
            app.mouse_hit_map.urls.clear();
            super::record_url_hits(
                &mut app,
                &"markdown-url".into(),
                body,
                None,
                Rect::new(3, 4, 40, 2),
                Alignment::Left,
            );
            let hits = &app.mouse_hit_map.urls;
            assert!(!hits.is_empty(), "{body}");
            assert!(hits.iter().all(|hit| hit.url == "https://example.com"));
            let lines = super::markdown_content_lines(body, &[], None, 40, &[]);
            let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 2));
            Paragraph::new(lines).render(buffer.area, &mut buffer);
            let painted = hits
                .iter()
                .map(|hit| buffer[(hit.area.x - 3, hit.area.y - 4)].symbol())
                .collect::<String>();
            assert_eq!(painted, expected_cells);
        }
    }

    #[test]
    fn labeled_link_hits_follow_wrapped_bidi_graphemes_and_override_url_labels() {
        use ratatui::{buffer::Buffer, layout::Alignment, style::Color, widgets::Widget};

        let destination = "https://example.com/_docs_?q=1&x=2!";
        for label in ["**אְב** 👩‍💻 阿丽", "https://label.test"] {
            let source = format!("before [{label}]({destination}) after");
            let visible_label = label.replace("**", "");
            let visible = format!("before {visible_label} after");
            for width in [4, 40] {
                let mut app = crate::app::test_support::TestApp::new();
                let area = Rect::new(3, 2, width, 12);
                super::record_url_hits(
                    &mut app,
                    &"label-cells".into(),
                    &source,
                    None,
                    area,
                    Alignment::Left,
                );
                let mut expected = Buffer::empty(Rect::new(0, 0, width, 12));
                ratatui::widgets::Paragraph::new(super::inline_content_lines(
                    &visible,
                    &[7..7 + visible_label.len()],
                    None,
                    width as usize,
                ))
                .alignment(Alignment::Left)
                .render(expected.area, &mut expected);
                let expected_cells = (0..12)
                    .flat_map(|y| (0..width).map(move |x| (x, y)))
                    .filter(|&(x, y)| expected[(x, y)].fg == Color::Blue)
                    .map(|(x, y)| Rect::new(x + 3, y + 2, 1, 1))
                    .collect::<Vec<_>>();
                assert!(!expected_cells.is_empty());
                assert_eq!(
                    app.mouse_hit_map
                        .urls
                        .iter()
                        .map(|hit| hit.area)
                        .collect::<Vec<_>>(),
                    expected_cells
                );
                assert!(
                    app.mouse_hit_map
                        .urls
                        .iter()
                        .all(|hit| hit.url == destination)
                );
            }
        }
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

/// Probe the exact wrapped/bidi paragraph into an offscreen buffer. Marking one
/// projected URL or labeled-link range as a mention changes only style, not geometry.
/// The actual message is rendered unchanged; the probe's blue cells become hits.
pub(super) fn record_url_hits(
    app: &mut App,
    id: &wr::MessageId,
    body: &str,
    status: Option<StatusLabel>,
    area: Rect,
    alignment: Alignment,
) {
    if area.is_empty() || matches!(status, Some(StatusLabel::Deleted)) {
        return;
    }
    let projection = crate::ui::markdown::Markdown::new(body, None).project(body);
    let body = projection.text.as_str();
    let mut targets = Vec::new();
    let mut search_from = 0;
    for url in crate::url::extract_openable_urls(body) {
        let Some(offset) = body[search_from..].find(&url) else {
            continue;
        };
        let start = search_from + offset;
        search_from = start + url.len();
        // A URL-looking label belongs to its explicit destination, not to the
        // URL extractor. Bare URLs and URLs in literal/code text keep that path.
        if projection.links.iter().any(|link| {
            link.ranges
                .iter()
                .any(|range| range.start < search_from && range.end > start)
        }) {
            continue;
        }
        targets.push((vec![start..search_from], url));
    }
    targets.extend(
        projection
            .links
            .into_iter()
            .map(|link| (link.ranges, link.url)),
    );
    for (ranges, url) in targets {
        let mut probe = Buffer::empty(Rect::new(0, 0, area.width, area.height));
        Paragraph::new(inline_content_lines(
            body,
            &ranges,
            status,
            area.width as usize,
        ))
        .alignment(alignment)
        .render(Rect::new(0, 0, area.width, area.height), &mut probe);
        for y in 0..area.height {
            for x in 0..area.width {
                if probe[(x, y)].fg == Color::Blue {
                    app.mouse_hit_map
                        .urls
                        .push(crate::app::mouse_input::UrlHit {
                            area: Rect::new(area.x + x, area.y + y, 1, 1),
                            id: id.clone(),
                            url: url.clone(),
                        });
                }
            }
        }
    }
}

fn spacing_after_message(
    index: usize,
    author_groups: &[AuthorGroupContext],
    selected: Option<usize>,
) -> usize {
    if author_groups[index].stack_count == 0 {
        return 0;
    }
    let older =
        (index + 1..author_groups.len()).find(|older| author_groups[*older].stack_count != 0);
    let has_older_neighbor = older.is_some();
    let continuation = !author_groups[index].starts_group();
    let selected_at_boundary = selected == Some(index) || selected.is_some() && selected == older;

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
    media_render_plan: &mut MediaRenderPlan,
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
        wr::MessageContent::ViewOnceUnavailable => ratatui::layout::Alignment::Left,
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

    let mut msg_area = message_content_area(msg_block.inner(area), is_selected);
    if author_group.stack_count > 1 {
        let [pile, detail] = Layout::vertical([
            Constraint::Length(author_group.stack_height() as u16),
            Constraint::Min(0),
        ])
        .areas(msg_area);
        let lines = vec![Line::raw(format!(
            "{} photos/videos",
            author_group.stack_count
        ))];
        Paragraph::new(lines).dark_gray().render(pile, buf);
        msg_area = detail;
    }
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
            let highlight_ranges = app
                .conversation_search
                .as_ref()
                .filter(|search| {
                    matches!(text_mode, MessageTextMode::Chat)
                        && app.focus_pane == FocusPane::Conversation
                        && app.conversation_mode == ConversationMode::MessageNavigation
                        && app.open_chat.as_ref() == Some(&search.chat)
                        && message.info.chat == search.chat
                        && !search.query.is_empty()
                        && !matches!(status, Some(StatusLabel::Deleted))
                })
                .map(|search| {
                    text.match_indices(&search.query)
                        .map(|(start, match_text)| start..start + match_text.len())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let lines = markdown_content_lines(
                text,
                &mention_ranges,
                status,
                content_area.width as usize,
                &highlight_ranges,
            );
            Paragraph::new(lines)
                .style(if is_pending {
                    Style::default().dim()
                } else {
                    Style::default()
                })
                .alignment(alignment)
                .render(content_area, buf);
            record_url_hits(app, &message.info.id, text, status, content_area, alignment);
        }
        wr::MessageContent::File(data) => {
            render_file(
                buf,
                &message.info.id,
                data,
                status,
                app,
                media_render_plan,
                content_area,
                render_image && author_group.stack_count <= 1,
                alignment,
                text_mode,
            );
            if author_group.stack_count > 1 {
                media_render_plan.record_stack_region(
                    message.info.id.clone(),
                    Rect::new(
                        content_area.x,
                        content_area.y,
                        content_area.width,
                        content_area.height.min(preview_height(&data.kind) as u16),
                    ),
                );
            }
            if is_pending && text_mode == MessageTextMode::Status {
                buf.set_style(content_area, Style::default().fg(Color::DarkGray).dim());
            }
        }
        wr::MessageContent::ViewOnceUnavailable => {
            Paragraph::new(inline_content_lines(
                wr::VIEW_ONCE_UNAVAILABLE_DESCRIPTION,
                &[],
                None,
                content_area.width as usize,
            ))
            .style(Style::default().dark_gray().dim())
            .alignment(alignment)
            .render(content_area, buf);
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

pub fn render_messages_with_plan(
    frame: &mut Frame,
    app: &mut App,
    media_render_plan: &mut MediaRenderPlan,
    visibility_plan: &mut VisibilityPlan,
    area: Rect,
) -> Option<()> {
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
        visibility_plan,
        MessageTextMode::Chat,
        media_render_plan,
    );
    let pending_started = app.message_list_phase_started();
    let pending_rows_rendered = render_pending_tail(
        frame,
        app,
        media_render_plan,
        optimistic_area,
        &optimistic_items,
        MessageTextMode::Chat,
    );
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
    media_render_plan: &mut MediaRenderPlan,
    area: Rect,
    items: &[wr::Message],
    text_mode: MessageTextMode,
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
            media_render_plan,
            false,
            text_mode,
        );
        rendered_rows += 1;
        y = y.saturating_add(heights[index] as u16);
    }
    rendered_rows
}

/// Read-only statuses of the opened status contact, rendered with the
/// same machinery as chats (media previews, timestamps, sender header).
pub fn render_status_messages_with_plan(
    frame: &mut Frame,
    app: &mut App,
    media_render_plan: &mut MediaRenderPlan,
    visibility_plan: &mut VisibilityPlan,
    area: Rect,
) {
    let Some(contact) = app.open_status_contact() else {
        return;
    };
    let ids = app.status_messages(&contact);
    render_status_items(frame, app, area, ids, media_render_plan, visibility_plan);
}

pub fn render_own_status_messages(
    frame: &mut Frame,
    app: &mut App,
    media_render_plan: &mut MediaRenderPlan,
    visibility_plan: &mut VisibilityPlan,
    area: Rect,
) {
    let pending = app.pending_own_status_messages();
    let pending_height = pending_tail_height(app, &pending, area.width as usize).min(area.height);
    let list_area = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(pending_height),
    );
    let pending_area = Rect::new(area.x, list_area.bottom(), area.width, pending_height);
    let ids = app.own_status_messages();
    render_status_items(
        frame,
        app,
        list_area,
        ids,
        media_render_plan,
        visibility_plan,
    );
    render_pending_tail(
        frame,
        app,
        media_render_plan,
        pending_area,
        &pending,
        MessageTextMode::Status,
    );
}

fn render_status_items(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    ids: Vec<wr::MessageId>,
    media_render_plan: &mut MediaRenderPlan,
    visibility_plan: &mut VisibilityPlan,
) {
    let items: Arc<[wr::MessageId]> = ids
        .into_iter()
        .rev()
        .filter(|id| app.messages.contains_key(id))
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
        visibility_plan,
        MessageTextMode::Status,
        media_render_plan,
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
    visibility_plan: &mut VisibilityPlan,
    text_mode: MessageTextMode,
    media_render_plan: &mut MediaRenderPlan,
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

    let author_groups: Arc<[AuthorGroupContext]> = if text_mode == MessageTextMode::Chat {
        message_stacks::project(app, &items, &author_groups, unread_count).into()
    } else {
        author_groups
    };
    app.mouse_hit_map.message_list = Some(list_area);
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
        visibility_plan,
        text_mode,
        media_render_plan,
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
