use crate::input_key::KeyCode;
use crate::ui::layout::composer_visual_layout_with_direction;

use super::*;
use crate::input_key::Key;

fn participant(name: &str, phone: &str) -> wr::GroupParticipant {
    wr::GroupParticipant {
        jid: format!("{phone}@s.whatsapp.net").into(),
        phone_number: format!("{phone}@s.whatsapp.net").into(),
        name: name.into(),
    }
}

fn participant_with_jid(name: &str, jid: &str, phone: &str) -> wr::GroupParticipant {
    wr::GroupParticipant {
        jid: jid.to_owned().into(),
        phone_number: phone.to_owned().into(),
        name: name.into(),
    }
}

#[test]
fn textarea_cursor_conversion_preserves_logical_units() {
    assert_eq!(
        logical_cursor_position(ratatui_textarea::DataCursor(2, 5)),
        (2, 5)
    );
}

#[test]
fn group_mentions_activate_filter_and_expand_on_submit() {
    let mut composer = Composer::default();
    composer.set_group_participants(vec![participant("Alice", "111"), participant("Bob", "222")]);
    composer.insert_text("hello @al");
    assert_eq!(composer.mention_picker_labels(), vec!["Alice"]);
    composer.confirm_mention();
    assert_eq!(composer.text(), "hello @Alice ");
    let outcome = composer.apply(ComposerAction::Submit);
    assert_eq!(outcome.text_messages(), vec!["hello @111 "]);
    assert!(matches!(
        &outcome,
        ComposerOutcome::Submit {
            display_text,
            display_mention_ranges,
            ..
        } if display_text.as_ref() == "hello @Alice "
            && display_mention_ranges == &vec![6.."hello @Alice".len()]
    ));
    assert!(composer.text().is_empty());
}

#[test]
fn lid_mentions_use_wire_jid_user_in_text_and_caption_tokens() {
    let participant =
        participant_with_jid("Alice", "987654321@lid", "5491112345678@s.whatsapp.net");
    let mut composer = Composer::default();
    composer.set_group_participants(vec![participant]);
    composer.insert_text("hello @al");
    composer.confirm_mention();

    let text_outcome = composer.apply(ComposerAction::Submit);
    assert!(matches!(
        &text_outcome,
        ComposerOutcome::Submit { messages, mentions, .. }
            if text_outcome.text_messages() == vec!["hello @987654321 "]
                && mentions == &vec![wr::Mention {
                    jid: "987654321@lid".to_owned().into(),
                    numeric_user: "987654321".into(),
                }]
    ));

    let mut composer = Composer::default();
    composer.set_group_participants(vec![participant_with_jid(
        "Alice",
        "987654321@lid",
        "5491112345678@s.whatsapp.net",
    )]);
    composer.insert_text("hello @al");
    composer.confirm_mention();
    composer.queue_attachment("photo.jpg".into(), wr::FileKind::Image);

    let outcome = composer.apply(ComposerAction::Submit);
    assert!(matches!(
        &outcome,
        ComposerOutcome::Submit { messages, mentions, .. }
            if messages.iter().any(|message| matches!(
                message,
                wr::MessageContent::File(file) if file.caption.as_deref() == Some("hello @987654321 ")
            ))
                && mentions == &vec![wr::Mention {
                    jid: "987654321@lid".to_owned().into(),
                    numeric_user: "987654321".into(),
                }]
    ));
}

#[test]
fn editing_a_selected_mention_discards_stale_semantic_metadata() {
    let mut composer = Composer::default();
    composer.set_group_participants(vec![participant("Alice", "111")]);
    composer.insert_text("@a");
    composer.confirm_mention();
    composer.replace_text("@Alix");
    let outcome = composer.apply(ComposerAction::Submit);
    assert_eq!(outcome.text_messages(), vec!["@Alix"]);
    assert!(matches!(outcome, ComposerOutcome::Submit { mentions, .. } if mentions.is_empty()));
}

#[test]
fn literal_tokens_before_selected_mentions_are_not_reinterpreted() {
    let mut composer = Composer::default();
    composer.set_group_participants(vec![participant("Alice", "111")]);
    composer.insert_text("email @support then @al");
    composer.confirm_mention();

    let outcome = composer.apply(ComposerAction::Submit);
    assert_eq!(outcome.text_messages(), vec!["email @support then @111 "]);
    assert!(
        matches!(outcome, ComposerOutcome::Submit { mentions, .. } if mentions == vec![wr::Mention { jid: "111@s.whatsapp.net".to_owned().into(), numeric_user: "111".into() }])
    );
}

#[test]
fn multiple_mentions_preserve_text_order_and_metadata_order() {
    let mut composer = Composer::default();
    composer.set_group_participants(vec![participant("Alice", "111"), participant("Bob", "222")]);
    composer.insert_text("@al");
    composer.confirm_mention();
    composer.insert_text(" and @bo");
    composer.confirm_mention();

    let outcome = composer.apply(ComposerAction::Submit);
    assert_eq!(outcome.text_messages(), vec!["@111  and @222 "]);
    assert!(
        matches!(outcome, ComposerOutcome::Submit { mentions, .. } if mentions.iter().map(|mention| mention.numeric_user.as_ref()).collect::<Vec<_>>() == vec!["111", "222"])
    );
}

#[test]
fn edits_before_a_mention_adjust_its_range_without_changing_identity() {
    let mut composer = Composer::default();
    composer.set_group_participants(vec![participant("Álice", "111")]);
    composer.insert_text("@á");
    composer.confirm_mention();
    composer.set_cursor_offset(0);
    composer.insert_text("prefix ");
    composer.set_cursor_offset(7);
    for _ in 0..7 {
        composer.apply(ComposerAction::Edit(Key::k(KeyCode::Backspace)));
    }

    let outcome = composer.apply(ComposerAction::Submit);
    assert_eq!(outcome.text_messages(), vec!["@111 "]);
    assert!(matches!(outcome, ComposerOutcome::Submit { mentions, .. } if mentions.len() == 1));
}

#[test]
fn all_group_members_remain_filterable_and_navigable() {
    let names = [
        "Alice", "Bob", "Carol", "Diana", "Eve", "Frank", "Grace", "Heidi",
    ];
    let mut composer = Composer::default();
    composer.set_group_participants(
        names
            .iter()
            .enumerate()
            .map(|(index, name)| participant(name, &(100 + index).to_string()))
            .collect(),
    );
    composer.insert_text("@");
    assert_eq!(composer.mention_picker_labels().len(), names.len());
    for (index, name) in names.iter().enumerate() {
        composer.replace_text(&format!("@{}", name.to_lowercase()));
        assert_eq!(composer.mention_picker_labels(), vec![name.to_string()]);
        composer.replace_text("@");
        composer.mention_picker.as_mut().unwrap().selected = index;
        assert_eq!(composer.mention_picker_selected(), index);
    }
    composer.mention_picker.as_mut().unwrap().selected = 0;
    for expected in 1..names.len() {
        composer.move_mention_selection(1);
        assert_eq!(composer.mention_picker_selected(), expected);
    }
}

#[test]
fn backspace_removes_complete_graphemes() {
    for source in ["e\u{301}", "👩‍💻"] {
        let mut composer = Composer::default();
        composer.insert_text(source);
        composer.apply(ComposerAction::Edit(Key::k(KeyCode::Backspace)));
        assert_eq!(composer.text(), "");
    }
}

#[test]
fn forced_rtl_movement_changes_logical_insertion_point_without_rewriting_source() {
    let mut composer = Composer::default();
    composer.insert_text("אבג");
    composer.apply_with_direction(
        ComposerAction::Edit(Key::k(KeyCode::Right)),
        ComposerDirection::Rtl,
        80,
    );
    composer.apply_with_direction(
        ComposerAction::Edit(Key::c('x')),
        ComposerDirection::Rtl,
        80,
    );
    assert_eq!(composer.text(), "אבxג");
}

#[test]
fn wrapped_up_down_preserves_preferred_visual_column() {
    let mut composer = Composer::default();
    composer.insert_text("abcdef");
    composer.apply_with_direction(
        ComposerAction::Edit(Key::k(KeyCode::Up)),
        ComposerDirection::Auto,
        3,
    );
    assert_eq!(logical_cursor_position(composer.input.cursor()), (0, 3));
    composer.apply_with_direction(
        ComposerAction::Edit(Key::k(KeyCode::Down)),
        ComposerDirection::Auto,
        3,
    );
    assert_eq!(logical_cursor_position(composer.input.cursor()), (0, 6));
}

#[test]
fn horizontal_movement_crosses_hard_line_boundaries() {
    let mut composer = Composer::default();
    composer.insert_text("ab\ncd");
    composer.set_cursor_offset(3);
    composer.apply(ComposerAction::Edit(Key::k(KeyCode::Left)));
    assert_eq!(logical_cursor_position(composer.input.cursor()), (0, 2));
}

#[test]
fn backspace_deletes_the_logical_preceding_grapheme() {
    let mut composer = Composer::default();
    composer.insert_text("אבג");
    composer.set_cursor_offset(2);
    composer.apply_with_direction(
        ComposerAction::Edit(Key::k(KeyCode::Backspace)),
        ComposerDirection::Rtl,
        20,
    );
    assert_eq!(composer.text(), "אג");
    assert_eq!(composer.cursor_offset(), 1);
}

#[test]
fn rendered_cursor_uses_the_stored_affinity() {
    let mut composer = Composer::default();
    composer.insert_text("אבג abc");
    composer.set_cursor_offset(3);
    composer.apply_with_direction(
        ComposerAction::Edit(Key::k(KeyCode::Right)),
        ComposerDirection::Auto,
        20,
    );
    let layout =
        composer_visual_layout_with_direction(composer.input.lines(), 20, ComposerDirection::Auto);
    assert_eq!(
        composer.visual_cursor(20, ComposerDirection::Auto),
        layout.cursor_with_affinity(
            logical_cursor_position(composer.input.cursor()),
            composer.caret_affinity,
        )
    );
}

#[test]
fn quote_and_mentions_are_submitted_together() {
    let mut composer = Composer::default();
    composer.set_group_participants(vec![participant("Alice", "111")]);
    composer.insert_text("@al");
    composer.confirm_mention();
    composer.quote = Some(wr::Message {
        info: wr::MessageInfo {
            id: "quoted".into(),
            chat: "123@g.us".to_owned().into(),
            sender: "222@s.whatsapp.net".to_owned().into(),
            mentions_self: false,
            timestamp: 0,
            is_from_me: false,
            quote_id: None,
            read_by: 0,
            forwarding: Default::default(),
        },
        message: wr::MessageContent::Text("quoted text".into()),
    });

    let outcome = composer.apply(ComposerAction::Submit);
    assert!(
        matches!(outcome, ComposerOutcome::Submit { quote: Some(_), mentions, .. } if mentions.len() == 1)
    );
}

#[test]
fn markdown_preview_cursor_uses_visible_columns_and_submit_keeps_source() {
    let mut composer = Composer::default();
    let source = "*one* tail";
    composer.insert_text(source);
    assert_eq!(composer.text(), source);
    assert_eq!(composer.visual_cursor(40, ComposerDirection::Auto), (0, 8));
    let outcome = composer.apply(ComposerAction::Submit);
    assert_eq!(outcome.text_messages(), vec![source]);
}

#[test]
fn labeled_link_preview_reveals_active_source_and_submits_original_destination() {
    use ratatui::style::Modifier;

    let source = "[Ejemplo](https://example.com) tail";
    let mut composer = Composer::default();
    composer.insert_text(source);
    composer.set_cursor_offset(3);
    assert_eq!(
        composer.visual_layout(ComposerDirection::Auto, 80).text(),
        source
    );
    assert_eq!(composer.text(), source);
    composer.set_cursor_offset(source.chars().count());
    let layout = composer.visual_layout(ComposerDirection::Auto, 80);
    assert_eq!(layout.text(), "Ejemplo tail");
    for span in layout.lines()[0].spans.iter().take("Ejemplo".len()) {
        assert!(span.style.add_modifier.contains(Modifier::UNDERLINED));
    }
    assert_eq!(composer.visual_cursor(80, ComposerDirection::Auto), (0, 12));
    assert_eq!(composer.text(), source);
    let outcome = composer.apply(ComposerAction::Submit);
    assert_eq!(outcome.text_messages(), vec![source]);
    assert!(matches!(
        &outcome,
        ComposerOutcome::Submit { display_text, .. } if display_text.as_ref() == source
    ));
    assert!(composer.text().is_empty());
}

#[test]
fn literal_markdown_and_bare_url_code_previews_keep_exact_submit_source() {
    for (source, preview) in [
        ("[enlace sin destino] tail", "[enlace sin destino] tail"),
        (
            "![imagen no soportada](https://example.com/image.png) tail",
            "![imagen no soportada](https://example.com/image.png) tail",
        ),
        (
            "https://example.com/_docs_ tail",
            "https://example.com/_docs_ tail",
        ),
        (
            "`[Ejemplo](https://example.com)` tail",
            "[Ejemplo](https://example.com) tail",
        ),
    ] {
        let mut composer = Composer::default();
        composer.insert_text(source);
        assert_eq!(
            composer.visual_layout(ComposerDirection::Auto, 80).text(),
            preview,
            "{source}"
        );
        assert_eq!(composer.text(), source);
        let outcome = composer.apply(ComposerAction::Submit);
        assert_eq!(outcome.text_messages(), vec![source]);
        assert!(matches!(
            &outcome,
            ComposerOutcome::Submit { display_text, .. } if display_text.as_ref() == source
        ));
        assert!(composer.text().is_empty());
    }
}

#[test]
fn incoming_url_examples_remain_raw_in_composer_preview_and_submit() {
    for source in [
        "https://github.com/Andiveli/wptui",
        "https://example.com/docs/installation",
        "https://example.com/",
        "https://docs.api.example.com:8443/a/install?q=one#part",
    ] {
        let mut composer = Composer::default();
        composer.insert_text(source);
        assert_eq!(
            composer.visual_layout(ComposerDirection::Auto, 80).text(),
            source
        );
        assert_eq!(composer.text(), source);
        let outcome = composer.apply(ComposerAction::Submit);
        assert_eq!(outcome.text_messages(), vec![source]);
        assert!(matches!(
            &outcome,
            ComposerOutcome::Submit { display_text, .. } if display_text.as_ref() == source
        ));
        assert!(composer.text().is_empty());
    }
}

#[test]
fn malformed_link_before_http_image_keeps_preview_and_submit_source() {
    let source = concat!(
        "[enlace sin destino]\n",
        "![imagen no soportada](https://example.com/image.png)\n",
        "[Ejemplo](https://example.com) tail"
    );
    let mut composer = Composer::default();
    composer.insert_text(source);
    assert_eq!(
        composer.visual_layout(ComposerDirection::Auto, 80).text(),
        concat!(
            "[enlace sin destino]\n",
            "![imagen no soportada](https://example.com/image.png)\n",
            "Ejemplo tail"
        )
    );
    assert_eq!(composer.text(), source);
    let outcome = composer.apply(ComposerAction::Submit);
    assert_eq!(outcome.text_messages(), vec![source]);
    assert!(matches!(
        &outcome,
        ComposerOutcome::Submit { display_text, .. } if display_text.as_ref() == source
    ));
    assert!(composer.text().is_empty());
}

#[test]
fn owner_three_space_blocks_preview_reveal_active_source_and_submit_unchanged() {
    use ratatui::style::Modifier;

    // Exact L18 source, including three ASCII spaces and the empty rows.
    let source = concat!(
        "# Encabezado 1\n",
        "   ## Encabezado 2\n",
        "   ### Encabezado 3\n\n",
        "   > Una cita de prueba\n\n",
        "   [Ejemplo](https://example.com)\n\n",
        "   [enlace sin destino]\n",
        "   ![imagen no soportada](https://example.com/image.png)"
    );
    let mut composer = Composer::default();
    composer.insert_text(source);
    assert_eq!(composer.text(), source);
    let layout = composer.visual_layout(ComposerDirection::Auto, 80);
    for (row, text, modifier) in [
        (0, "Encabezado 1", Modifier::BOLD),
        (1, "Encabezado 2", Modifier::BOLD),
        (2, "Encabezado 3", Modifier::BOLD),
        (4, "Una cita de prueba", Modifier::ITALIC | Modifier::DIM),
        (6, "Ejemplo", Modifier::UNDERLINED),
    ] {
        let line = &layout.lines()[row];
        // Composer indentation geometry is not part of this regression.
        assert_eq!(line.to_string().trim_start(), text, "preview row {row}");
        for span in &line.spans {
            if !span.content.trim().is_empty() {
                assert!(span.style.add_modifier.contains(modifier), "row {row}");
            }
        }
    }
    for row in [3, 5, 7] {
        assert_eq!(layout.lines()[row].to_string(), "");
    }
    for (row, literal) in [
        (8, "[enlace sin destino]"),
        (9, "![imagen no soportada](https://example.com/image.png)"),
    ] {
        let line = &layout.lines()[row];
        assert_eq!(line.to_string().trim_start(), literal);
        assert!(
            line.spans
                .iter()
                .all(|span| span.style.add_modifier.is_empty())
        );
    }

    // Moving into each block must reveal its original marker for editing,
    // without changing the stored text or inventing an indentation policy.
    for fragment in [
        "## Encabezado 2",
        "### Encabezado 3",
        "> Una cita de prueba",
    ] {
        let start = source.find(fragment).unwrap();
        composer.set_cursor_offset(source[..start + 1].chars().count());
        assert!(
            composer
                .visual_layout(ComposerDirection::Auto, 80)
                .text()
                .contains(fragment),
            "active block must reveal {fragment:?}"
        );
        assert_eq!(composer.text(), source);
    }
    let outcome = composer.apply(ComposerAction::Submit);
    assert_eq!(outcome.text_messages(), vec![source]);
    assert!(matches!(
        &outcome,
        ComposerOutcome::Submit { display_text, .. } if display_text.as_ref() == source
    ));
    assert!(composer.text().is_empty());
}

#[test]
fn live_markdown_reveals_only_active_fragment_and_navigation_reaches_source() {
    use ratatui::style::Modifier;
    let mut composer = Composer::default();
    let source = "*one* **two** tail";
    composer.insert_text(source);
    let layout = composer.visual_layout(ComposerDirection::Auto, 40);
    assert_eq!(layout.text(), "one two tail");
    assert!(
        layout.lines()[0]
            .spans
            .iter()
            .any(|s| s.content == "o" && s.style.add_modifier.contains(Modifier::BOLD))
    );
    composer.set_cursor_offset(2);
    assert_eq!(
        composer.visual_layout(ComposerDirection::Auto, 40).text(),
        "*one* two tail"
    );
    assert_eq!(composer.visual_cursor(40, ComposerDirection::Auto), (0, 2));
    composer.set_cursor_offset(source.chars().count());
    for _ in 0..source.chars().count() + 4 {
        composer.apply_with_direction(
            ComposerAction::Edit(Key::k(KeyCode::Left)),
            ComposerDirection::Auto,
            40,
        );
    }
    assert_eq!(composer.cursor_offset(), 0);
    assert_eq!(composer.text(), source);
}

#[test]
fn markdown_preview_keeps_unicode_rtl_wrapping_and_fence_source_rows() {
    for (source, cursor, expected) in [
        ("**阿丽 👩‍💻** tail", 0, "**阿丽 👩‍💻** tail"),
        ("**שלום** tail", 12, "שלום tail"),
        ("```rs\n*literal*\n```\ntail", 25, "*literal*\ntail"),
    ] {
        let mut composer = Composer::default();
        composer.insert_text(source);
        composer.set_cursor_offset(cursor.min(source.chars().count()));
        let layout = composer.visual_layout(ComposerDirection::Auto, 40);
        // RTL rows are already visually reordered; source storage is not.
        if !source.contains("שלום") {
            assert_eq!(layout.text(), expected);
        }
        assert_eq!(composer.text(), source);
        for width in [4, 12, 40] {
            let layout = composer.visual_layout(ComposerDirection::Auto, width);
            let caret = composer.visual_cursor(width, ComposerDirection::Auto);
            assert!(caret.0 <= layout.row_count());
            assert!(caret.1 < width as usize);
        }
        let outcome = composer.apply(ComposerAction::Submit);
        assert_eq!(outcome.text_messages(), vec![source]);
    }
}

#[test]
fn plain_composer_and_raw_layout_keep_their_existing_geometry() {
    for source in ["plain text", "אבג abc", "阿丽 👩‍💻\nnext"] {
        for width in [4, 20] {
            let mut composer = Composer::default();
            composer.insert_text(source);
            let raw = composer_visual_layout_with_direction(
                composer.input.lines(),
                width,
                ComposerDirection::Auto,
            );
            assert_eq!(
                composer
                    .visual_layout(ComposerDirection::Auto, width)
                    .lines(),
                raw.lines()
            );
            assert_eq!(
                composer.visual_cursor(width, ComposerDirection::Auto),
                raw.cursor(logical_cursor_position(composer.input.cursor()))
            );
        }
    }
    let raw =
        composer_visual_layout_with_direction(&["**raw**".into()], 40, ComposerDirection::Auto);
    assert_eq!(raw.text(), "**raw**");
}

#[test]
fn inactive_closing_fence_whitespace_does_not_add_preview_rows() {
    for suffix in ["", " ", "  "] {
        let source = format!("```rs\nx\n```{suffix}\ntail");
        let mut composer = Composer::default();
        composer.insert_text(&source);
        let layout = composer.visual_layout(ComposerDirection::Auto, 40);
        assert_eq!(layout.row_count(), 2, "{source:?}");
        assert_eq!(layout.text().lines().last(), Some("tail"));
        assert_eq!(
            composer.visual_cursor(40, ComposerDirection::Auto),
            (1, 4),
            "{source:?}"
        );
        assert_eq!(composer.text(), source);
        let outcome = composer.apply(ComposerAction::Submit);
        assert_eq!(outcome.text_messages(), vec![source.as_str()]);
    }
}

#[test]
fn hiding_a_delimiter_preserves_its_following_combining_mark() {
    let source = "*a*\u{301} tail";
    let expected = "a\u{301} tail";
    let mut composer = Composer::default();
    composer.insert_text(source);
    let projection = crate::ui::markdown::Markdown::new(source, None).project(source);
    assert_eq!(projection.text, expected);
    let layout = composer.visual_layout(ComposerDirection::Auto, 40);
    assert_eq!(layout.text(), expected);
    // Check real terminal cells as well as the source/display string.
    let mut buffer = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 40, 1));
    use ratatui::widgets::Widget;
    ratatui::widgets::Paragraph::new(layout.lines()).render(buffer.area, &mut buffer);
    assert_eq!(buffer[(0, 0)].symbol(), "a\u{301}");
    assert_eq!(composer.text(), source);
    let outcome = composer.apply(ComposerAction::Submit);
    assert_eq!(outcome.text_messages(), vec![source]);
}
