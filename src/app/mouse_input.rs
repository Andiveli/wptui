use ratatui::{
    crossterm::event::{MouseButton, MouseEvent, MouseEventKind},
    layout::{Position, Rect},
};
use whatsrust as wr;

use super::App;
use super::actions::{ConversationMode, FocusPane, Section, StatusCompositionState};

/// Coordinates from the last completed draw, not a second approximation of layout.
#[derive(Default)]
pub(crate) struct MouseHitMap {
    pub(crate) section: Section,
    pub(crate) chat: Option<wr::JID>,
    pub(crate) status_contact: Option<wr::JID>,
    pub(crate) status_composition: StatusCompositionState,
    pub(crate) chat_list: Option<Rect>,
    pub(crate) message_list: Option<Rect>,
    pub(crate) messages: Vec<MessageHit>,
    /// Remaining rows before the oldest message reaches the top, if measured.
    pub(crate) scroll_up_remaining: Option<usize>,
}

pub(crate) struct MessageHit {
    pub(crate) area: Rect,
    pub(crate) id: wr::MessageId,
}

impl App<'_> {
    pub(crate) fn on_mouse_event(&mut self, event: MouseEvent) {
        if !self.mouse_capture_enabled
            || self.pending_logout
            || self.logout_in_progress
            || self.shortcut_popup
            || self.leader_menu.is_some()
            || self.contextual_menu.is_some()
            || self.attachment_viewer.is_some()
            || self.url_picker.is_some()
            || self.file_picker.is_some()
            || self.share_picker.is_some()
            || self.reaction_picker.is_some()
            || self.message_menu.is_some()
            || self.composer.mention_picker_active()
            || self.mouse_hit_map.section != self.selected_section
            || self.mouse_hit_map.chat != self.open_chat()
            || self.mouse_hit_map.status_contact != self.open_status_contact()
            || self.mouse_hit_map.status_composition != self.status_composition
        {
            return;
        }
        let point = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let down = event.kind == MouseEventKind::ScrollDown;
                if self
                    .mouse_hit_map
                    .chat_list
                    .is_some_and(|area| area.contains(point))
                {
                    self.move_chat_selection(if down { 1 } else { -1 });
                } else if self
                    .mouse_hit_map
                    .message_list
                    .is_some_and(|area| area.contains(point))
                {
                    let offset = &mut self.message_list_state.offset;
                    if down {
                        *offset = offset.saturating_sub(3);
                    } else {
                        *offset = offset.saturating_add(
                            self.mouse_hit_map.scroll_up_remaining.unwrap_or(3).min(3),
                        );
                    }
                    self.message_list_state.viewport_anchor = None;
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(id) = self
                    .mouse_hit_map
                    .messages
                    .iter()
                    .find(|hit| hit.area.contains(point))
                    .map(|hit| hit.id.clone())
                    .filter(|id| self.messages.contains_key(id))
                {
                    self.message_list_state.set_selected_message(id);
                    self.focus_pane = FocusPane::Conversation;
                    if self.conversation_mode == ConversationMode::ComposerEditing {
                        self.conversation_mode = ConversationMode::MessageNavigation;
                    }
                }
            }
            _ => {} // Terminal-dependent Shift+drag remains native selection.
        }
    }
}
