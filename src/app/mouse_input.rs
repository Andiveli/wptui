use ratatui::{
    crossterm::event::{MouseButton, MouseEvent, MouseEventKind},
    layout::{Position, Rect},
};
use whatsrust as wr;

use super::actions::{AppAction, ConversationMode, FocusPane, Section, StatusCompositionState};
use super::{App, CommunityNavigationRow, ContactRow};

/// Coordinates from the last completed draw, not a second approximation of layout.
#[derive(Default)]
pub(crate) struct MouseHitMap {
    pub(crate) section: Section,
    pub(crate) chat: Option<wr::JID>,
    pub(crate) status_contact: Option<wr::JID>,
    pub(crate) status_composition: StatusCompositionState,
    pub(crate) chat_list: Option<Rect>,
    pub(crate) section_rows: Vec<SectionHit>,
    pub(crate) contact_rows: Vec<ContactHit>,
    pub(crate) message_list: Option<Rect>,
    pub(crate) picker: Option<PickerHit>,
    pub(crate) messages: Vec<MessageHit>,
    pub(crate) urls: Vec<UrlHit>,
    pub(crate) media: Vec<MediaHit>,
    /// Remaining rows before the oldest message reaches the top, if measured.
    pub(crate) scroll_up_remaining: Option<usize>,
}

pub(crate) struct SectionHit {
    pub(crate) area: Rect,
    pub(crate) section: Option<Section>, // None is the Logout rail entry.
}

#[derive(Clone)]
pub(crate) enum ContactTarget {
    Chat(ContactRow),
    Status(wr::JID),
    Community(CommunityNavigationRow),
}

pub(crate) struct ContactHit {
    pub(crate) area: Rect,
    pub(crate) index: usize,
    pub(crate) target: ContactTarget,
}

pub(crate) struct MessageHit {
    pub(crate) area: Rect,
    pub(crate) id: wr::MessageId,
}

pub(crate) struct UrlHit {
    pub(crate) area: Rect,
    pub(crate) id: wr::MessageId,
    pub(crate) url: String,
}

pub(crate) struct MediaHit {
    pub(crate) area: Rect,
    pub(crate) id: wr::MessageId,
    pub(crate) kind: wr::FileKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerKind {
    Share,
    Url,
    File,
}

#[derive(Clone, Copy)]
pub(crate) struct PickerHit {
    pub(crate) kind: PickerKind,
    pub(crate) list: Rect,
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
        let active_picker = if self.file_picker.is_some() {
            Some(PickerKind::File)
        } else if self.share_picker.is_some() {
            Some(PickerKind::Share)
        } else if self.url_picker.is_some() {
            Some(PickerKind::Url)
        } else {
            None
        };
        if let Some(kind) = active_picker {
            // The last draw owns the coordinates. No event may reach the pane
            // behind a modal, including its border or an unpainted picker.
            let Some(hit) = self.mouse_hit_map.picker else {
                return;
            };
            if hit.kind != kind || !hit.list.contains(point) {
                return;
            }
            let delta = match event.kind {
                MouseEventKind::ScrollDown => 1,
                MouseEventKind::ScrollUp => -1,
                _ => return,
            };
            match kind {
                PickerKind::Share => self.move_share_picker(delta),
                PickerKind::Url => self.move_url_picker(delta),
                PickerKind::File => {
                    if let Some(picker) = self.file_picker.as_mut() {
                        picker.move_selection(delta);
                    }
                }
            }
            return;
        }
        if self.mouse_hit_map.picker.is_some() {
            return; // A picker closed since the last draw; do not use stale base hits.
        }
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
                if let Some(section) = self
                    .mouse_hit_map
                    .section_rows
                    .iter()
                    .find(|hit| hit.area.contains(point))
                    .map(|hit| hit.section)
                {
                    self.focus_pane = FocusPane::SectionRail;
                    if let Some(section) = section {
                        self.selected_section = section;
                        self.rail_on_logout = false;
                        self.dispatch_action(AppAction::FocusPane(FocusPane::ChatList));
                    } else {
                        self.rail_on_logout = true;
                        self.begin_logout_confirmation();
                    }
                    return;
                }
                if let Some((index, target)) = self
                    .mouse_hit_map
                    .contact_rows
                    .iter()
                    .find(|hit| hit.area.contains(point))
                    .map(|hit| (hit.index, hit.target.clone()))
                {
                    let still_painted = match &target {
                        ContactTarget::Chat(row) => self
                            .cached_contact_view()
                            .0
                            .get(index)
                            .is_some_and(|current| current == row),
                        ContactTarget::Status(jid) => self.status_contacts.get(index) == Some(jid),
                        ContactTarget::Community(row) => {
                            self.community_navigation_rows()
                                .into_iter()
                                .filter(|row| !matches!(row, CommunityNavigationRow::Separator))
                                .nth(index)
                                .as_ref()
                                == Some(row)
                        }
                    };
                    if still_painted {
                        match target {
                            ContactTarget::Status(_) => self.status_selection.select(Some(index)),
                            ContactTarget::Chat(_) | ContactTarget::Community(_) => {
                                self.chat_list_state.select(Some(index));
                            }
                        }
                        self.focus_pane = FocusPane::ChatList;
                        self.dispatch_action(AppAction::OpenChat);
                    }
                    return;
                }
                if let Some(id) = self
                    .mouse_hit_map
                    .messages
                    .iter()
                    .find(|hit| hit.area.contains(point))
                    .map(|hit| hit.id.clone())
                    .filter(|id| self.messages.contains_key(id))
                {
                    self.message_list_state.set_selected_message(id.clone());
                    self.focus_pane = FocusPane::Conversation;
                    if self.conversation_mode == ConversationMode::ComposerEditing {
                        self.conversation_mode = ConversationMode::MessageNavigation;
                    }
                    let mut activated = false;
                    if !self.selected_message_is_deleted() {
                        if let Some(url) = self
                            .mouse_hit_map
                            .urls
                            .iter()
                            .find(|hit| hit.id == id && hit.area.contains(point))
                            .map(|hit| hit.url.clone())
                        {
                            self.launch_url(&url);
                            activated = true;
                        } else if let Some(kind) = self
                            .mouse_hit_map
                            .media
                            .iter()
                            .find(|hit| hit.id == id && hit.area.contains(point))
                            .map(|hit| hit.kind.clone())
                        {
                            match kind {
                                wr::FileKind::Image | wr::FileKind::Video => {
                                    self.open_attachment_viewer()
                                }
                                wr::FileKind::Document => self.open_selected_document(),
                                wr::FileKind::Audio => self.plan_selected_media_launch(),
                                wr::FileKind::Sticker => {} // There is no sticker viewer.
                            }
                            activated = true;
                        }
                    }
                    if !activated && self.selected_section == Section::Status {
                        self.dispatch_action(AppAction::ViewMessage);
                    } else if !activated
                        && self.messages.get(&id).is_some_and(|message| {
                            matches!(&message.message, wr::MessageContent::Text(_))
                        })
                    {
                        self.dispatch_action(AppAction::OpenContextualActions);
                    }
                }
            }
            _ => {} // Terminal-dependent Shift+drag remains native selection.
        }
    }
}
