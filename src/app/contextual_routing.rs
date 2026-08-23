use crate::app::contextual_actions::{ContextualAction, ContextualMenuRow, contextual_key_action};
use crate::input_key::{Key, KeyCode};

/// Routing is scoped to the immutable model currently presented to the user.
pub fn route_contextual_key(rows: &[ContextualMenuRow], key: &Key) -> Option<ContextualAction> {
    let KeyCode::Char(character) = key.code else {
        return None;
    };
    contextual_key_action(rows, character)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::{FocusPane, Section};
    use crate::app::contextual_actions::{ContextualContext, RowStyle, contextual_menu_rows};

    #[test]
    fn q_routes_to_quit_in_each_contextual_menu() {
        for focus in [
            FocusPane::SectionRail,
            FocusPane::ChatList,
            FocusPane::Conversation,
        ] {
            for section in [Section::Chats, Section::Status] {
                let rows = contextual_menu_rows(ContextualContext {
                    focus,
                    section,
                    has_selected_message: false,
                    selected_text: false,
                    selected_message_is_informational: false,
                    has_reference: false,
                    attach_blocked: false,
                });
                let quit = rows
                    .iter()
                    .find(|row| row.display_shortcut == 'q')
                    .expect("quit shortcut");
                assert_eq!(quit.row_style, RowStyle::Enabled);
                assert_eq!(
                    route_contextual_key(&rows, &Key::c('q')),
                    Some(ContextualAction::Quit)
                );
            }
        }
    }
}
