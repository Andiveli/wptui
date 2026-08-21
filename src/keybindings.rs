use crate::app::actions::{AppAction, FocusPane};
use crate::input_key::{Key, KeyCode, KeyModifiers};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingImplementation {
    Implemented,
    Planned,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaderBinding {
    pub key: Key,
    pub action: AppAction,
    pub label: &'static str,
    pub implementation: BindingImplementation,
}

pub fn leader_bindings() -> Vec<LeaderBinding> {
    vec![
        LeaderBinding {
            key: Key::c('1'),
            action: AppAction::ToggleSectionRail,
            label: "Toggle section rail",
            implementation: BindingImplementation::Implemented,
        },
        LeaderBinding {
            key: Key::c('2'),
            action: AppAction::ToggleChatList,
            label: "Toggle chat list",
            implementation: BindingImplementation::Implemented,
        },
        LeaderBinding {
            key: Key::c('a'),
            action: AppAction::OpenContextualActions,
            label: "Contextual actions",
            implementation: BindingImplementation::Implemented,
        },
        LeaderBinding {
            key: Key::c('o'),
            action: AppAction::PlannedLeaderAction("WhatsApp options"),
            label: "WhatsApp options",
            implementation: BindingImplementation::Planned,
        },
        LeaderBinding {
            key: Key::c('s'),
            action: AppAction::PlannedLeaderAction("TUI settings"),
            label: "TUI settings",
            implementation: BindingImplementation::Planned,
        },
    ]
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SequenceResolution {
    Partial,
    Complete(AppAction),
    Cancelled,
}

pub fn default_bindings() -> Vec<(Vec<Key>, AppAction)> {
    let mut bindings = leader_bindings()
        .into_iter()
        .map(|binding| (vec![Key::c(' '), binding.key], binding.action))
        .collect::<Vec<_>>();
    bindings.extend(vec![
        (vec![Key::ctrl('q')], AppAction::Quit),
        (vec![Key::ctrl_shift('O')], AppAction::Logout),
        (vec![Key::ctrl_shift('L')], AppAction::ToggleLogs),
        (vec![Key::ctrl_shift('l')], AppAction::ToggleLogs),
        (
            vec![Key::c('1')],
            AppAction::FocusPane(FocusPane::SectionRail),
        ),
        (vec![Key::c('2')], AppAction::FocusPane(FocusPane::ChatList)),
        (
            vec![Key::c('3')],
            AppAction::FocusPane(FocusPane::Conversation),
        ),
        (vec![Key::c('?')], AppAction::ToggleShortcutPopup),
        (vec![Key::c('h')], AppAction::FocusPrevious),
        (vec![Key::c('l')], AppAction::FocusNext),
        (vec![Key::c('j')], AppAction::SelectNext),
        (vec![Key::c('k')], AppAction::SelectPrevious),
        (vec![Key::c('g'), Key::c('g')], AppAction::JumpTop),
        (vec![Key::c('g'), Key::c('r')], AppAction::GoToReference),
        (vec![Key::c('G')], AppAction::JumpBottom),
        (vec![Key::ctrl('d')], AppAction::HalfPageDown),
        (vec![Key::ctrl('u')], AppAction::HalfPageUp),
        (vec![Key::c('i')], AppAction::InsertMode),
        (vec![Key::c('y')], AppAction::CopyMessage),
        (vec![Key::c('r')], AppAction::ReactMessage),
        (vec![Key::c('s')], AppAction::ShareMessage),
        (vec![Key::c('R')], AppAction::ReplyMessage),
        (vec![Key::c('P')], AppAction::ReplyPrivately),
        (vec![Key::c('d')], AppAction::DeleteMessage),
        (vec![Key::c('e')], AppAction::EditMessage),
        (vec![Key::c('o')], AppAction::OpenMessage),
        (vec![Key::c('a')], AppAction::AttachFile),
        (vec![Key::c('x')], AppAction::DownloadMessage),
        (vec![Key::c('v')], AppAction::ViewMessage),
    ]);
    bindings
}

pub fn format_key(key: &Key) -> String {
    let name = match key.code {
        KeyCode::Char(' ') => "Space".to_owned(),
        KeyCode::Enter => "Enter".to_owned(),
        KeyCode::Char('?') => "?".to_owned(),
        KeyCode::Char(c) if c.is_ascii_uppercase() => c.to_string(),
        KeyCode::Char(c) => c.to_string(),
        code => format!("{code:?}"),
    };
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        format!("Ctrl+{name}")
    } else if key.modifiers.contains(KeyModifiers::ALT) {
        format!("Alt+{name}")
    } else if key.modifiers.contains(KeyModifiers::SHIFT)
        && !matches!(key.code, KeyCode::Char(c) if c.is_ascii_uppercase())
    {
        format!("Shift+{name}")
    } else {
        name
    }
}

pub fn canonical_shortcuts() -> Vec<(String, String)> {
    default_bindings()
        .into_iter()
        .map(|(keys, action)| {
            (
                keys.iter().map(format_key).collect::<Vec<_>>().join("+"),
                format!("{action:?}"),
            )
        })
        .collect()
}

pub fn resolve_sequence(keys: &[Key]) -> SequenceResolution {
    let mut partial = false;
    for (binding, action) in default_bindings() {
        if keys.len() > binding.len()
            || !keys.iter().zip(binding.iter()).all(|(key, expected)| {
                matches_sequence_binding(key, expected, binding[0].code == KeyCode::Char(' '))
            })
        {
            continue;
        }
        if keys.len() == binding.len() {
            return SequenceResolution::Complete(action);
        }
        partial = true;
    }
    if partial {
        SequenceResolution::Partial
    } else {
        SequenceResolution::Cancelled
    }
}

pub fn matches_binding(actual: &Key, expected: &Key) -> bool {
    let code_matches = actual.code == expected.code
        || matches!((actual.code, expected.code), (KeyCode::Char(actual), KeyCode::Char(expected)) if expected.is_ascii_uppercase() && actual == expected.to_ascii_lowercase() && actual_key_has_shift(actual));
    let modifiers_match = actual
        .modifiers
        .intersection(KeyModifiers::CONTROL | KeyModifiers::ALT)
        == expected
            .modifiers
            .intersection(KeyModifiers::CONTROL | KeyModifiers::ALT);
    let shift_match = if matches!(expected.code, KeyCode::Char('?')) {
        actual.modifiers.contains(KeyModifiers::SHIFT)
    } else {
        actual.modifiers.contains(KeyModifiers::SHIFT)
            == expected.modifiers.contains(KeyModifiers::SHIFT)
    };
    code_matches && modifiers_match && shift_match
}

fn matches_sequence_binding(actual: &Key, expected: &Key, is_leader: bool) -> bool {
    if is_leader && expected.code == KeyCode::Char(' ') {
        return matches_binding(actual, expected);
    }
    if is_leader && matches!(expected.code, KeyCode::Char(c) if c.is_ascii_lowercase()) {
        return leader_alias_matches(actual, expected);
    }
    matches_binding(actual, expected)
}

pub fn matches_leader_binding(actual: &Key, expected: &Key) -> bool {
    leader_alias_matches(actual, expected)
}

fn leader_alias_matches(actual: &Key, expected: &Key) -> bool {
    if matches!(expected.code, KeyCode::Char(c) if c.is_ascii_lowercase())
        && (actual.code == expected.code
            || matches!(actual.code, KeyCode::Char(c) if c == expected.code_char().to_ascii_uppercase()))
        && (actual.code != expected.code || actual.modifiers.contains(KeyModifiers::SHIFT))
    {
        return actual
            .modifiers
            .intersection(KeyModifiers::CONTROL | KeyModifiers::ALT)
            == expected
                .modifiers
                .intersection(KeyModifiers::CONTROL | KeyModifiers::ALT);
    }
    matches_binding(actual, expected)
}

fn actual_key_has_shift(key: char) -> bool {
    key.is_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn leader_resolution_and_shift_labels_are_canonical() {
        assert_eq!(
            resolve_sequence(&[Key::c(' ')]),
            SequenceResolution::Partial
        );
        assert_eq!(
            resolve_sequence(&[Key::c(' '), Key::c('1')]),
            SequenceResolution::Complete(AppAction::ToggleSectionRail)
        );
        assert_eq!(
            resolve_sequence(&[Key::c(' '), Key::c('o')]),
            SequenceResolution::Complete(AppAction::PlannedLeaderAction("WhatsApp options"))
        );
        assert_eq!(format_key(&Key::c('?')), "Shift+?");
        assert_eq!(format_key(&Key::c('A')), "A");
    }

    #[test]
    fn direct_lowercase_attach_binding_remains_canonical() {
        assert!(
            default_bindings()
                .iter()
                .any(|(keys, action)| keys == &[Key::c('a')] && *action == AppAction::AttachFile)
        );
    }

    #[test]
    fn leader_a_accepts_both_terminal_uppercase_representations() {
        let expected = Key::c('a');
        for actual in [
            Key {
                code: KeyCode::Char('A'),
                modifiers: KeyModifiers::SHIFT,
            },
            Key {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::SHIFT,
            },
        ] {
            assert!(matches_leader_binding(&actual, &expected));
            assert_eq!(
                resolve_sequence(&[Key::c(' '), actual]),
                SequenceResolution::Complete(AppAction::OpenContextualActions)
            );
        }
    }

    #[test]
    fn lowercase_g_does_not_match_jump_bottom() {
        assert!(!matches_binding(&Key::c('g'), &Key::c('G')));
        assert_eq!(
            resolve_sequence(&[Key::c('g')]),
            SequenceResolution::Partial
        );
        assert_eq!(
            resolve_sequence(&[Key::c('G')]),
            SequenceResolution::Complete(AppAction::JumpBottom)
        );
    }
}
