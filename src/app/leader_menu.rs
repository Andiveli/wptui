use crate::app::actions::AppAction;
use crate::app::contextual_actions::{
    AvailabilityFacts, RowStyle, evaluate_availability, row_style,
};
use crate::input_key::Key;
use crate::keybindings::{BindingImplementation, format_key, leader_bindings};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaderMenuContext {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaderMenuRow {
    pub key: Key,
    pub display_shortcut: String,
    pub display_label: String,
    pub action_token: AppAction,
    pub row_style: RowStyle,
    pub reason: Option<&'static str>,
}

pub fn build_leader_menu(_context: LeaderMenuContext) -> Vec<LeaderMenuRow> {
    let mut rows = Vec::new();
    for binding in leader_bindings() {
        let key = binding.key;
        if rows.iter().any(|row: &LeaderMenuRow| row.key == key) {
            continue;
        }
        let availability = evaluate_availability(
            match binding.implementation {
                BindingImplementation::Implemented => {
                    crate::app::contextual_actions::ImplementationStatus::Implemented
                }
                BindingImplementation::Planned => {
                    crate::app::contextual_actions::ImplementationStatus::Planned
                }
            },
            if matches!(binding.action, AppAction::OpenContextualActions) {
                None
            } else {
                Some(crate::app::contextual_actions::ContextualAction::Starred)
            },
            AvailabilityFacts {
                contextual: None,
                contextual_activatable: matches!(binding.action, AppAction::OpenContextualActions),
            },
        );
        rows.push(LeaderMenuRow {
            display_shortcut: format!("Space+{}", format_key(&key)),
            display_label: binding.label.to_owned(),
            key,
            action_token: binding.action,
            row_style: row_style(availability),
            reason: availability.reason,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contextual_route_stays_enabled_when_children_are_unavailable() {
        let rows = build_leader_menu(LeaderMenuContext {});
        assert!(
            rows.iter()
                .any(|row| row.display_shortcut == "Space+a" && row.row_style == RowStyle::Enabled)
        );
    }
}
