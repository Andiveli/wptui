use crate::app::contextual_actions::{ContextualAction, ContextualMenuRow, contextual_key_action};
use crate::input_key::{Key, KeyCode};

/// Routing is scoped to the immutable model currently presented to the user.
pub fn route_contextual_key(rows: &[ContextualMenuRow], key: &Key) -> Option<ContextualAction> {
    let KeyCode::Char(character) = key.code else {
        return None;
    };
    contextual_key_action(rows, character)
}
