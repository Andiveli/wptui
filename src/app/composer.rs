mod vim;

use std::{collections::HashSet, ops::Range, sync::Arc};

use ratatui_textarea::{CursorMove, TextArea};
use unicode_segmentation::UnicodeSegmentation;
use whatsrust as wr;

use crate::app::actions::ComposerAction;
use crate::app::preferences::ComposerDirection;
use crate::input_key::{Key, KeyCode};
use crate::ui::layout::{
    BoundaryAffinity, ComposerVisualLayout, HorizontalDirection,
    composer_visual_layout_with_direction,
};
#[derive(Clone, Debug)]
pub struct PendingAttachment {
    pub path: Arc<str>,
    pub kind: wr::FileKind,
}

impl PendingAttachment {
    pub fn new(path: Arc<str>, kind: wr::FileKind) -> Self {
        Self { path, kind }
    }

    pub fn display_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

#[derive(Debug)]
pub enum ComposerOutcome {
    Idle,
    Submit {
        messages: Vec<wr::MessageContent>,
        quote: Option<wr::Message>,
        mentions: Vec<wr::Mention>,
        mention_ranges: Vec<Range<usize>>,
        display_text: Arc<str>,
        display_mention_ranges: Vec<Range<usize>>,
        draft: Option<ComposerDraft>,
    },
}

#[derive(Clone, Debug)]
pub struct ComposerDraft {
    text: String,
    quote: Option<wr::Message>,
    mentions: Vec<MentionMark>,
    pending: Vec<PendingAttachment>,
}

impl ComposerOutcome {
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    pub fn text_messages(&self) -> Vec<&str> {
        match self {
            Self::Idle => Vec::new(),
            Self::Submit { messages, .. } => messages
                .iter()
                .filter_map(|message| match message {
                    wr::MessageContent::Text(text) => Some(text.as_ref()),
                    wr::MessageContent::File(_) | wr::MessageContent::ViewOnceUnavailable => None,
                })
                .collect(),
        }
    }

    pub fn file_messages(&self) -> Vec<&wr::FileContent> {
        match self {
            Self::Idle => Vec::new(),
            Self::Submit { messages, .. } => messages
                .iter()
                .filter_map(|message| match message {
                    wr::MessageContent::File(file) => Some(file),
                    wr::MessageContent::Text(_) | wr::MessageContent::ViewOnceUnavailable => None,
                })
                .collect(),
        }
    }
}

pub struct Composer<'a> {
    pub input: TextArea<'a>,
    pub quote: Option<wr::Message>,
    pub pending: Vec<PendingAttachment>,
    pub(crate) status_draft: Option<ComposerDraft>,
    chat_draft_before_status: Option<ComposerDraft>,
    pub blocked: bool,
    participants: Vec<wr::GroupParticipant>,
    mention_picker: Option<MentionPicker>,
    mentions: Vec<MentionMark>,
    caret_affinity: Option<BoundaryAffinity>,
    preferred_visual_column: Option<usize>,
    pending_vim_g: bool,
    pending_vim_y: bool,
}

fn logical_cursor_position(cursor: ratatui_textarea::DataCursor) -> (usize, usize) {
    (cursor.0, cursor.1)
}

#[derive(Clone, Debug)]
struct MentionPicker {
    start: usize,
    query: String,
    selected: usize,
}
#[derive(Clone, Debug)]
struct MentionMark {
    start: usize,
    end: usize,
    participant: wr::GroupParticipant,
}

impl Default for Composer<'_> {
    fn default() -> Self {
        let mut input = TextArea::default();
        input.set_placeholder_text("Type a message...");

        Self {
            input,
            quote: None,
            pending: Vec::new(),
            status_draft: None,
            chat_draft_before_status: None,
            blocked: false,
            participants: Vec::new(),
            mention_picker: None,
            mentions: Vec::new(),
            caret_affinity: None,
            preferred_visual_column: None,
            pending_vim_g: false,
            pending_vim_y: false,
        }
    }
}

impl Composer<'_> {
    pub fn apply(&mut self, action: ComposerAction) -> ComposerOutcome {
        self.apply_with_direction(action, ComposerDirection::Auto, 80)
    }

    pub(crate) fn apply_with_direction(
        &mut self,
        action: ComposerAction,
        direction: ComposerDirection,
        width: u16,
    ) -> ComposerOutcome {
        if self.blocked {
            return ComposerOutcome::Idle;
        }
        match action {
            ComposerAction::Submit => self.submit(),
            ComposerAction::InsertNewline => {
                self.input.insert_newline();
                self.caret_affinity = None;
                self.preferred_visual_column = None;
                ComposerOutcome::Idle
            }
            ComposerAction::Edit(key) => {
                let before = self.text();
                self.apply_edit_key(&key, direction, width);
                let after = self.text();
                self.reconcile_mentions(&before, &after);
                self.refresh_mention_picker();
                ComposerOutcome::Idle
            }
            ComposerAction::RemoveLastAttachment => {
                self.pending.pop();
                ComposerOutcome::Idle
            }
            ComposerAction::CancelReply => {
                self.quote = None;
                ComposerOutcome::Idle
            }
            ComposerAction::StartEdit | ComposerAction::Paste => ComposerOutcome::Idle,
        }
    }

    fn apply_edit_key(&mut self, key: &Key, direction: ComposerDirection, width: u16) {
        match key.code {
            KeyCode::Left | KeyCode::Right => {
                let layout = self.visual_layout(direction, width);
                let movement = if key.code == KeyCode::Left {
                    HorizontalDirection::Left
                } else {
                    HorizontalDirection::Right
                };
                let target = layout.move_horizontal(
                    logical_cursor_position(self.input.cursor()),
                    self.caret_affinity,
                    movement,
                );
                self.move_to_visual_caret(target);
                self.preferred_visual_column = None;
            }
            KeyCode::Up | KeyCode::Down => {
                let layout = self.visual_layout(direction, width);
                let movement = if key.code == KeyCode::Up {
                    HorizontalDirection::Left
                } else {
                    HorizontalDirection::Right
                };
                let current = layout.visual_caret(
                    logical_cursor_position(self.input.cursor()),
                    self.caret_affinity,
                );
                let preferred = self.preferred_visual_column.or(Some(current.visual_column));
                let target = layout.move_vertical(
                    logical_cursor_position(self.input.cursor()),
                    self.caret_affinity,
                    movement,
                    preferred,
                );
                self.preferred_visual_column = preferred;
                self.move_to_visual_caret(target);
            }
            KeyCode::Backspace => self.delete_previous_grapheme(),
            _ => {
                self.input
                    .input(crate::app::composer_input_mapping::textarea_input(key));
                self.caret_affinity = None;
                self.preferred_visual_column = None;
            }
        }
    }

    fn visual_layout(&self, direction: ComposerDirection, width: u16) -> ComposerVisualLayout {
        composer_visual_layout_with_direction(self.input.lines(), width, direction)
    }

    pub(crate) fn visual_cursor(&self, width: u16, direction: ComposerDirection) -> (usize, usize) {
        self.visual_layout(direction, width).cursor_with_affinity(
            logical_cursor_position(self.input.cursor()),
            self.caret_affinity,
        )
    }

    fn move_to_visual_caret(&mut self, caret: crate::ui::layout::ComposerCaret) {
        self.input.move_cursor(CursorMove::Jump(
            caret.logical_row.min(u16::MAX as usize) as u16,
            caret.logical_column.min(u16::MAX as usize) as u16,
        ));
        self.caret_affinity = Some(caret.affinity);
    }

    fn delete_previous_grapheme(&mut self) {
        let offset = self.cursor_offset();
        if offset == 0 {
            return;
        }
        let mut text = self.text();
        let (row, column) = logical_cursor_position(self.input.cursor());
        let removed = if column > 0 {
            let line = &self.input.lines()[row];
            let byte_end = line
                .char_indices()
                .nth(column)
                .map_or(line.len(), |(index, _)| index);
            let (start, _) = line[..byte_end]
                .grapheme_indices(true)
                .next_back()
                .unwrap_or((0, ""));
            line[..byte_end][start..].chars().count()
        } else {
            1
        };
        let start_char = offset.saturating_sub(removed);
        let start_byte = text
            .char_indices()
            .nth(start_char)
            .map_or(text.len(), |(index, _)| index);
        let end_byte = text
            .char_indices()
            .nth(offset)
            .map_or(text.len(), |(index, _)| index);
        text.replace_range(start_byte..end_byte, "");
        self.replace_input_contents(&text, start_char);
        self.caret_affinity = None;
        self.preferred_visual_column = None;
    }

    fn replace_input_contents(&mut self, text: &str, cursor_offset: usize) {
        self.input.select_all();
        self.input.delete_next_char();
        self.input.insert_str(text);
        self.set_cursor_offset(cursor_offset);
    }

    pub fn insert_text(&mut self, text: &str) {
        self.caret_affinity = None;
        self.preferred_visual_column = None;
        if self.blocked {
            return;
        }
        let before = self.text();
        self.input.insert_str(text);
        let after = self.text();
        self.reconcile_mentions(&before, &after);
        self.refresh_mention_picker();
    }

    pub fn replace_text(&mut self, text: &str) {
        self.caret_affinity = None;
        self.preferred_visual_column = None;
        let before = self.text();
        self.clear_text();
        self.input.insert_str(text);
        self.reconcile_mentions(&before, text);
        self.refresh_mention_picker();
    }

    pub fn clear_text(&mut self) {
        self.caret_affinity = None;
        self.preferred_visual_column = None;
        self.input.select_all();
        self.input.delete_next_char();
    }

    pub fn text(&self) -> String {
        self.input.lines().join("\n")
    }

    pub fn queue_attachment(&mut self, path: Arc<str>, kind: wr::FileKind) {
        if self.blocked {
            return;
        }
        self.pending.push(PendingAttachment::new(path, kind));
    }

    pub fn set_blocked(&mut self, blocked: bool) {
        self.blocked = blocked;
        if blocked {
            self.clear_text();
            self.quote = None;
            self.pending.clear();
        }
    }

    pub fn set_group_participants(&mut self, participants: Vec<wr::GroupParticipant>) {
        self.participants = deduplicate_group_participants(participants);
        self.refresh_mention_picker();
    }

    pub fn mention_picker_active(&self) -> bool {
        self.mention_picker.is_some()
    }

    pub fn mention_picker_labels(&self) -> Vec<String> {
        self.mention_picker
            .as_ref()
            .map_or_else(Vec::new, |picker| {
                self.mention_candidates(&picker.query)
                    .into_iter()
                    .map(|p| p.name.to_string())
                    .collect()
            })
    }

    pub fn mention_picker_selected(&self) -> usize {
        self.mention_picker
            .as_ref()
            .map_or(0, |picker| picker.selected)
    }

    pub fn move_mention_selection(&mut self, delta: isize) {
        let Some(query) = self
            .mention_picker
            .as_ref()
            .map(|picker| picker.query.clone())
        else {
            return;
        };
        let count = self.mention_candidates(&query).len();
        let Some(picker) = self.mention_picker.as_mut() else {
            return;
        };
        if count > 0 {
            picker.selected =
                (picker.selected as isize + delta).rem_euclid(count as isize) as usize;
        }
    }

    pub fn cancel_mention_picker(&mut self) {
        self.mention_picker = None;
    }

    pub fn confirm_mention(&mut self) {
        let Some(picker) = self.mention_picker.take() else {
            return;
        };
        let candidates = self.mention_candidates(&picker.query);
        let Some(participant) = candidates.get(picker.selected).cloned() else {
            return;
        };
        let replacement = format!("@{} ", participant.name);
        let mut chars: Vec<char> = self.text().chars().collect();
        let end = picker.start + 1 + picker.query.chars().count();
        chars.splice(picker.start..end, replacement.chars());
        let text: String = chars.into_iter().collect();
        self.replace_text(&text);
        self.set_cursor_offset(picker.start + replacement.chars().count());
        self.mentions.push(MentionMark {
            start: picker.start,
            end: picker.start + replacement.trim_end().chars().count(),
            participant,
        });
        self.refresh_mention_picker();
    }

    pub fn is_empty(&self) -> bool {
        self.input.lines().iter().all(String::is_empty) && self.pending.is_empty()
    }

    fn submit(&mut self) -> ComposerOutcome {
        let draft = ComposerDraft {
            text: self.text(),
            quote: self.quote.clone(),
            mentions: self.mentions.clone(),
            pending: self.pending.clone(),
        };
        let (expanded, mentions, mention_ranges, display_mention_ranges) = self.expanded_text();
        let display_text: Arc<str> = self.text().into();
        let text: Arc<str> = expanded.into();
        if text.trim().is_empty() && self.pending.is_empty() {
            return ComposerOutcome::Idle;
        }

        let messages = if self.pending.is_empty() {
            vec![wr::MessageContent::Text(text)]
        } else {
            self.pending
                .drain(..)
                .enumerate()
                .map(|(index, attachment)| {
                    wr::MessageContent::File(wr::FileContent {
                        kind: attachment.kind,
                        path: attachment.path,
                        file_id: "".into(),
                        caption: (index == 0).then(|| text.clone()),
                    })
                })
                .collect()
        };

        self.input.select_all();
        self.input.delete_next_char();
        self.mentions.clear();
        self.mention_picker = None;

        ComposerOutcome::Submit {
            messages,
            quote: self.quote.take(),
            mentions,
            mention_ranges,
            display_text,
            display_mention_ranges,
            draft: Some(draft),
        }
    }

    pub(crate) fn begin_status_context(&mut self) {
        if self.chat_draft_before_status.is_some() {
            return;
        }
        self.chat_draft_before_status = Some(ComposerDraft {
            text: self.text(),
            quote: self.quote.clone(),
            mentions: self.mentions.clone(),
            pending: self.pending.clone(),
        });
        self.replace_text("");
        self.quote = None;
        self.mentions.clear();
        self.mention_picker = None;
        self.pending.clear();
    }

    pub(crate) fn end_status_context(&mut self) {
        let Some(draft) = self.chat_draft_before_status.take() else {
            return;
        };
        self.replace_text("");
        self.quote = None;
        self.mentions.clear();
        self.mention_picker = None;
        self.pending = draft.pending.clone();
        self.restore_text_draft(draft);
    }

    pub(crate) fn restore_status_draft(&mut self) {
        self.restore_status_draft_after(0);
    }

    pub(crate) fn restore_status_draft_after(&mut self, sent: usize) {
        if let Some(mut draft) = self.status_draft.take() {
            if !draft.pending.is_empty() {
                draft.pending.drain(..sent.min(draft.pending.len()));
                if sent > 0 {
                    draft.text.clear();
                    draft.mentions.clear();
                    draft.quote = None;
                }
            } else if sent > 0 {
                return;
            }
            self.pending = draft.pending.clone();
            self.restore_text_draft(draft);
        }
    }

    pub(crate) fn restore_text_draft(&mut self, draft: ComposerDraft) {
        self.replace_text(&draft.text);
        self.quote = draft.quote;
        self.mentions = draft.mentions;
        self.refresh_mention_picker();
    }

    fn cursor_offset(&self) -> usize {
        let cursor = self.input.cursor();
        let row = cursor.0;
        let column = cursor.1;
        self.input
            .lines()
            .iter()
            .take(row)
            .map(|line| line.chars().count() + 1)
            .sum::<usize>()
            + column
    }

    fn set_cursor_offset(&mut self, offset: usize) {
        let mut remaining = offset;
        for (row, line) in self.input.lines().iter().enumerate() {
            let length = line.chars().count();
            if remaining <= length {
                self.input
                    .move_cursor(CursorMove::Jump(row as u16, remaining as u16));
                return;
            }
            remaining = remaining.saturating_sub(length + 1);
        }
        self.input.move_cursor(CursorMove::Jump(u16::MAX, u16::MAX));
    }

    fn reconcile_mentions(&mut self, before: &str, after: &str) {
        let old: Vec<char> = before.chars().collect();
        let new: Vec<char> = after.chars().collect();
        let mut prefix = 0;
        while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
            prefix += 1;
        }
        let mut old_suffix = old.len();
        let mut new_suffix = new.len();
        while old_suffix > prefix
            && new_suffix > prefix
            && old[old_suffix - 1] == new[new_suffix - 1]
        {
            old_suffix -= 1;
            new_suffix -= 1;
        }
        let delta = new_suffix as isize - old_suffix as isize;
        for mark in &mut self.mentions {
            if old_suffix <= mark.start {
                mark.start = (mark.start as isize + delta) as usize;
                mark.end = (mark.end as isize + delta) as usize;
            } else if prefix >= mark.end {
                // The edit happened after this mark.
            } else {
                mark.start = usize::MAX;
                mark.end = 0;
            }
        }
        self.mentions.retain(|mark| {
            mark.start != usize::MAX
                && mark.end <= new.len()
                && new[mark.start..mark.end].iter().collect::<String>()
                    == format!("@{}", mark.participant.name)
        });
    }

    fn mention_candidates(&self, query: &str) -> Vec<wr::GroupParticipant> {
        let query = query.to_lowercase();
        self.participants
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }

    fn refresh_mention_picker(&mut self) {
        let offset = self.cursor_offset();
        let chars: Vec<char> = self.text().chars().collect();
        let mut start = offset;
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        if start < offset && chars.get(start) == Some(&'@') {
            let query: String = chars[start + 1..offset].iter().collect();
            if query
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                if !self.mention_candidates(&query).is_empty() {
                    self.mention_picker = Some(MentionPicker {
                        start,
                        query,
                        selected: 0,
                    });
                    return;
                }
            }
        }
        self.mention_picker = None;
    }

    fn expanded_text(
        &self,
    ) -> (
        String,
        Vec<wr::Mention>,
        Vec<Range<usize>>,
        Vec<Range<usize>>,
    ) {
        let text = self.text();
        let mut marks = self.mentions.clone();
        marks.sort_by_key(|mark| mark.start);
        let mut output = String::new();
        let mut last = 0;
        let mut mentions = Vec::new();
        let mut mention_ranges = Vec::new();
        let mut display_mention_ranges = Vec::new();
        for mark in marks {
            if mark.end > text.chars().count() {
                continue;
            }
            let chars: Vec<char> = text.chars().collect();
            if mark.start >= mark.end || chars[mark.start] != '@' {
                continue;
            }
            output.extend(chars[last..mark.start].iter());
            let wire_user = mark
                .participant
                .jid
                .0
                .split('@')
                .next()
                .unwrap_or("")
                .to_owned();
            let mention_start = output.len();
            let display_start = text
                .char_indices()
                .nth(mark.start)
                .map_or(text.len(), |(offset, _)| offset);
            let display_end = text
                .char_indices()
                .nth(mark.end)
                .map_or(text.len(), |(offset, _)| offset);
            output.push('@');
            output.push_str(&wire_user);
            mention_ranges.push(mention_start..output.len());
            display_mention_ranges.push(display_start..display_end);
            last = mark.end;
            mentions.push(wr::Mention {
                jid: mark.participant.jid,
                numeric_user: wire_user.into(),
            });
        }
        let chars: Vec<char> = text.chars().collect();
        output.extend(chars[last..].iter());
        (output, mentions, mention_ranges, display_mention_ranges)
    }
}

fn deduplicate_group_participants(
    participants: Vec<wr::GroupParticipant>,
) -> Vec<wr::GroupParticipant> {
    let mut result = Vec::with_capacity(participants.len());
    for participant in participants {
        let participant_ids = participant_identity_ids(&participant);
        if let Some(existing) = result.iter_mut().find(|existing| {
            participant_identity_ids(existing)
                .iter()
                .any(|identity| participant_ids.contains(identity))
        }) {
            if existing.jid.0.is_empty() {
                existing.jid = participant.jid.clone();
            }
            if existing.phone_number.0.is_empty() {
                existing.phone_number = participant.phone_number.clone();
            }
            if existing.name.is_empty() {
                existing.name = participant.name.clone();
            }
        } else {
            result.push(participant);
        }
    }
    result
}

fn participant_identity_ids(participant: &wr::GroupParticipant) -> HashSet<String> {
    [
        participant.jid.0.as_ref(),
        participant.phone_number.0.as_ref(),
    ]
    .into_iter()
    .filter(|jid| !jid.is_empty())
    .map(ToOwned::to_owned)
    .collect()
}

#[cfg(test)]
mod tests;
