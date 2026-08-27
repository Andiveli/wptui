use crate::app::actions::{AppAction, FocusPane, Section};
use crate::app::contextual_actions::{ContextualAction, ContextualMenuRow, RowStyle};
use crate::app::contextual_routing::route_contextual_key;
use crate::app::input_mapping::{
    attachment_viewer_action, file_picker_navigation_action, file_picker_search_action,
    message_menu_action, reaction_picker_action, share_picker_action, url_picker_action,
};
use crate::input_key::{Key, KeyCode};
use crate::keybindings::matches_leader_binding;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InputRoute {
    Action(AppAction),
    DismissLeader,
    DismissContextual,
    ActivateLeader,
    ActivateContextualShortcut(ContextualAction),
    ActivateContextualSelection,
    MoveLeader(isize),
    MoveContextual(isize),
    Ignore,
}

pub(crate) enum ModalContext<'a> {
    ShortcutPopup,
    Leader(&'a [crate::app::leader_menu::LeaderMenuRow]),
    Contextual(&'a [ContextualMenuRow]),
    AttachmentViewer,
    UrlPicker,
    FilePickerSearch,
    FilePickerNavigation,
    SharePicker,
    ReactionPicker,
    MessageMenu,
}

pub(crate) fn route_modal_key(key: &Key, context: ModalContext<'_>) -> InputRoute {
    match context {
        ModalContext::ShortcutPopup => {
            if *key == Key::k(KeyCode::Esc) || *key == Key::c('?') {
                InputRoute::DismissLeader
            } else {
                InputRoute::Ignore
            }
        }
        ModalContext::Leader(rows) => {
            if *key == Key::k(KeyCode::Esc) {
                InputRoute::DismissLeader
            } else if *key == Key::k(KeyCode::Enter) {
                InputRoute::ActivateLeader
            } else if *key == Key::k(KeyCode::Char('j')) || *key == Key::k(KeyCode::Down) {
                InputRoute::MoveLeader(1)
            } else if *key == Key::k(KeyCode::Char('k')) || *key == Key::k(KeyCode::Up) {
                InputRoute::MoveLeader(-1)
            } else if rows.iter().any(|row| {
                row.row_style == RowStyle::Enabled && matches_leader_binding(key, &row.key)
            }) {
                InputRoute::ActivateLeader
            } else {
                InputRoute::Ignore
            }
        }
        ModalContext::Contextual(rows) => {
            if *key == Key::k(KeyCode::Esc) {
                InputRoute::DismissContextual
            } else if *key == Key::k(KeyCode::Enter) {
                InputRoute::ActivateContextualSelection
            } else if *key == Key::k(KeyCode::Char('j')) || *key == Key::k(KeyCode::Down) {
                InputRoute::MoveContextual(1)
            } else if *key == Key::k(KeyCode::Char('k')) || *key == Key::k(KeyCode::Up) {
                InputRoute::MoveContextual(-1)
            } else if let Some(action) = route_contextual_key(rows, key) {
                InputRoute::ActivateContextualShortcut(action)
            } else {
                InputRoute::Ignore
            }
        }
        ModalContext::AttachmentViewer => mapped(attachment_viewer_action(key)),
        ModalContext::UrlPicker => mapped(url_picker_action(key)),
        ModalContext::FilePickerSearch => mapped(file_picker_search_action(key)),
        ModalContext::FilePickerNavigation => mapped(file_picker_navigation_action(key)),
        ModalContext::SharePicker => mapped(share_picker_action(key)),
        ModalContext::ReactionPicker => mapped(reaction_picker_action(key)),
        ModalContext::MessageMenu => mapped(message_menu_action(key)),
    }
}

fn mapped(action: Option<AppAction>) -> InputRoute {
    action.map_or(InputRoute::Ignore, InputRoute::Action)
}

pub(crate) fn route_context_key(
    key: &Key,
    focus_pane: FocusPane,
    selected_section: Section,
    community_detail: bool,
    rail_on_logout: bool,
) -> Option<AppAction> {
    if focus_pane == FocusPane::Conversation && selected_section == Section::Status {
        return match key.code {
            KeyCode::Esc => Some(AppAction::CloseStatusPane),
            KeyCode::Enter => Some(AppAction::ViewMessage),
            _ => None,
        };
    }
    if focus_pane == FocusPane::ChatList && community_detail && *key == Key::k(KeyCode::Esc) {
        return None;
    }
    if focus_pane == FocusPane::SectionRail
        && rail_on_logout
        && *key == Key::k(KeyCode::Enter)
    {
        return None;
    }
    if *key == Key::k(KeyCode::Enter) {
        return Some(match focus_pane {
            FocusPane::SectionRail => AppAction::FocusPane(FocusPane::ChatList),
            FocusPane::ChatList => AppAction::OpenChat,
            FocusPane::Conversation => AppAction::OpenContextualActions,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::Section;

    #[test]
    fn status_enter_precedes_generic_enter() {
        assert_eq!(
            route_context_key(&Key::k(KeyCode::Enter), FocusPane::Conversation, Section::Status, false, false),
            Some(AppAction::ViewMessage)
        );
    }

    #[test]
    fn generic_enter_maps_to_the_focused_pane() {
        assert_eq!(
            route_context_key(&Key::k(KeyCode::Enter), FocusPane::ChatList, Section::Chats, false, false),
            Some(AppAction::OpenChat)
        );
    }

    #[test]
    fn picker_routes_only_supported_keys() {
        assert_eq!(route_modal_key(&Key::k(KeyCode::Down), ModalContext::ReactionPicker), InputRoute::Action(AppAction::ReactionNext));
        assert_eq!(route_modal_key(&Key::k(KeyCode::Tab), ModalContext::ReactionPicker), InputRoute::Ignore);
    }

    #[test]
    fn contextual_enter_preserves_selected_row_activation() {
        let rows = [ContextualMenuRow {
            action_token: ContextualAction::Quit,
            display_label: "Quit",
            display_shortcut: 'q',
            row_style: RowStyle::Enabled,
            reason: None,
        }];
        assert_eq!(
            route_modal_key(&Key::k(KeyCode::Enter), ModalContext::Contextual(&rows)),
            InputRoute::ActivateContextualSelection
        );
    }
}
