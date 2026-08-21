use super::actions::{FocusPane, Section};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImplementationStatus {
    Implemented,
    Planned,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionAvailability {
    pub implementation: ImplementationStatus,
    pub capability: bool,
    pub permission: bool,
    pub reason: Option<&'static str>,
}

impl ActionAvailability {
    pub const fn planned() -> Self {
        Self {
            implementation: ImplementationStatus::Planned,
            capability: false,
            permission: false,
            reason: Some("not implemented"),
        }
    }

    pub const fn enabled() -> Self {
        Self {
            implementation: ImplementationStatus::Implemented,
            capability: true,
            permission: true,
            reason: None,
        }
    }

    pub const fn disabled(reason: &'static str) -> Self {
        Self {
            implementation: ImplementationStatus::Implemented,
            capability: true,
            permission: false,
            reason: Some(reason),
        }
    }

    pub const fn activatable(self) -> bool {
        matches!(self.implementation, ImplementationStatus::Implemented)
            && self.capability
            && self.permission
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowStyle {
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AvailabilityFacts {
    pub contextual: Option<ContextualContext>,
    pub contextual_activatable: bool,
}

pub fn evaluate_availability(
    implementation: ImplementationStatus,
    action: Option<ContextualAction>,
    facts: AvailabilityFacts,
) -> ActionAvailability {
    if implementation == ImplementationStatus::Planned {
        return ActionAvailability::planned();
    }
    if action == Some(ContextualAction::Copy)
        && !facts
            .contextual
            .is_some_and(|context| context.selected_text)
    {
        return ActionAvailability::disabled("text message required");
    }
    if action == Some(ContextualAction::GoToReference)
        && !facts
            .contextual
            .is_some_and(|context| context.has_reference)
    {
        return ActionAvailability::disabled("reference unavailable");
    }
    if action == Some(ContextualAction::Attach)
        && facts
            .contextual
            .is_some_and(|context| context.attach_blocked)
    {
        return ActionAvailability::disabled("attachment unavailable");
    }
    if action.is_some_and(|action| {
        matches!(
            action,
            ContextualAction::DeleteForEveryone
                | ContextualAction::Share
                | ContextualAction::ReplyPrivately
                | ContextualAction::React
                | ContextualAction::Reply
                | ContextualAction::Open
                | ContextualAction::ViewAttachment
        )
    }) && facts
        .contextual
        .is_some_and(|context| !context.has_selected_message || context.section == Section::Status)
    {
        return ActionAvailability::disabled("not available in this view");
    }
    if action.is_none() && !facts.contextual_activatable {
        return ActionAvailability::disabled("no activatable actions");
    }
    ActionAvailability::enabled()
}

pub fn row_style(availability: ActionAvailability) -> RowStyle {
    if availability.capability
        && availability.permission
        && availability.implementation == ImplementationStatus::Implemented
    {
        RowStyle::Enabled
    } else {
        RowStyle::Disabled
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextualAction {
    Starred,
    ReadAll,
    NewGroup,
    NewCommunity,
    StatusPrivacy,
    MarkRead,
    ViewContact,
    Pin,
    Mute,
    Hide,
    DeleteForMe,
    DeleteForEveryone,
    Share,
    Star,
    ReplyPrivately,
    Copy,
    React,
    Reply,
    Attach,
    Open,
    ViewAttachment,
    GoToReference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextualActionMetadata {
    pub action: ContextualAction,
    pub label: &'static str,
    pub shortcut: char,
    pub scope: ContextualScope,
    pub implementation: ImplementationStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextualScope {
    SectionRail,
    ChatList,
    Conversation,
    StatusChatList,
    AnyConversation,
}

pub const CONTEXTUAL_ACTION_METADATA: [ContextualActionMetadata; 22] = [
    m(
        ContextualAction::Starred,
        "Starred",
        's',
        ContextualScope::SectionRail,
        ImplementationStatus::Planned,
    ),
    m(
        ContextualAction::ReadAll,
        "Read all",
        'r',
        ContextualScope::SectionRail,
        ImplementationStatus::Planned,
    ),
    m(
        ContextualAction::NewGroup,
        "New group",
        'g',
        ContextualScope::SectionRail,
        ImplementationStatus::Planned,
    ),
    m(
        ContextualAction::NewCommunity,
        "New community",
        'c',
        ContextualScope::SectionRail,
        ImplementationStatus::Planned,
    ),
    m(
        ContextualAction::StatusPrivacy,
        "Status privacy",
        'p',
        ContextualScope::SectionRail,
        ImplementationStatus::Planned,
    ),
    m(
        ContextualAction::MarkRead,
        "Mark as read/unread",
        'm',
        ContextualScope::ChatList,
        ImplementationStatus::Planned,
    ),
    m(
        ContextualAction::ViewContact,
        "View contact/group",
        'v',
        ContextualScope::ChatList,
        ImplementationStatus::Planned,
    ),
    m(
        ContextualAction::Pin,
        "Pin/unpin",
        'p',
        ContextualScope::ChatList,
        ImplementationStatus::Planned,
    ),
    m(
        ContextualAction::Mute,
        "Mute/unmute",
        'u',
        ContextualScope::ChatList,
        ImplementationStatus::Planned,
    ),
    m(
        ContextualAction::Hide,
        "Hide",
        'h',
        ContextualScope::StatusChatList,
        ImplementationStatus::Planned,
    ),
    m(
        ContextualAction::DeleteForMe,
        "Delete for me",
        'd',
        ContextualScope::Conversation,
        ImplementationStatus::Planned,
    ),
    m(
        ContextualAction::DeleteForEveryone,
        "Delete for everyone",
        'D',
        ContextualScope::Conversation,
        ImplementationStatus::Implemented,
    ),
    m(
        ContextualAction::Share,
        "Share",
        's',
        ContextualScope::Conversation,
        ImplementationStatus::Implemented,
    ),
    m(
        ContextualAction::Star,
        "Star/unstar",
        'S',
        ContextualScope::Conversation,
        ImplementationStatus::Planned,
    ),
    m(
        ContextualAction::ReplyPrivately,
        "Reply privately",
        'P',
        ContextualScope::Conversation,
        ImplementationStatus::Implemented,
    ),
    m(
        ContextualAction::Copy,
        "Copy",
        'y',
        ContextualScope::Conversation,
        ImplementationStatus::Implemented,
    ),
    m(
        ContextualAction::React,
        "React",
        'r',
        ContextualScope::Conversation,
        ImplementationStatus::Implemented,
    ),
    m(
        ContextualAction::Reply,
        "Reply",
        'R',
        ContextualScope::Conversation,
        ImplementationStatus::Implemented,
    ),
    m(
        ContextualAction::Attach,
        "Attach",
        'a',
        ContextualScope::Conversation,
        ImplementationStatus::Implemented,
    ),
    m(
        ContextualAction::Open,
        "Open link/document",
        'o',
        ContextualScope::Conversation,
        ImplementationStatus::Implemented,
    ),
    m(
        ContextualAction::ViewAttachment,
        "Attachment viewer",
        'v',
        ContextualScope::Conversation,
        ImplementationStatus::Implemented,
    ),
    m(
        ContextualAction::GoToReference,
        "Go to reference",
        'g',
        ContextualScope::Conversation,
        ImplementationStatus::Implemented,
    ),
];

const fn m(
    action: ContextualAction,
    label: &'static str,
    shortcut: char,
    scope: ContextualScope,
    implementation: ImplementationStatus,
) -> ContextualActionMetadata {
    ContextualActionMetadata {
        action,
        label,
        shortcut,
        scope,
        implementation,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextualContext {
    pub focus: FocusPane,
    pub section: Section,
    pub has_selected_message: bool,
    pub selected_text: bool,
    pub has_reference: bool,
    pub attach_blocked: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextualMenuRow {
    pub action_token: ContextualAction,
    pub display_label: &'static str,
    pub display_shortcut: char,
    pub row_style: RowStyle,
    pub reason: Option<&'static str>,
}

impl ContextualMenuRow {}

pub fn contextual_menu_rows(context: ContextualContext) -> Vec<ContextualMenuRow> {
    CONTEXTUAL_ACTION_METADATA
        .iter()
        .filter(|meta| applies(meta.scope, context))
        .map(|meta| {
            let availability = evaluate_availability(
                meta.implementation,
                Some(meta.action),
                AvailabilityFacts {
                    contextual: Some(context),
                    contextual_activatable: false,
                },
            );
            ContextualMenuRow {
                action_token: meta.action,
                display_label: meta.label,
                display_shortcut: meta.shortcut,
                row_style: row_style(availability),
                reason: availability.reason,
            }
        })
        .collect()
}

fn applies(scope: ContextualScope, context: ContextualContext) -> bool {
    match scope {
        ContextualScope::SectionRail => context.focus == FocusPane::SectionRail,
        ContextualScope::ChatList => {
            context.focus == FocusPane::ChatList && context.section != Section::Status
        }
        ContextualScope::StatusChatList => {
            context.focus == FocusPane::ChatList && context.section == Section::Status
        }
        ContextualScope::Conversation | ContextualScope::AnyConversation => {
            context.focus == FocusPane::Conversation
        }
    }
}

pub fn contextual_key_action(rows: &[ContextualMenuRow], key: char) -> Option<ContextualAction> {
    rows.iter()
        .find(|row| {
            row.display_shortcut == key
                || (row.display_shortcut.is_ascii_uppercase()
                    && row.display_shortcut.to_ascii_lowercase() == key)
        })
        .map(|row| row.action_token)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_catalog_preserves_colliding_shortcuts() {
        let rows = contextual_menu_rows(ContextualContext {
            focus: FocusPane::Conversation,
            section: Section::Chats,
            has_selected_message: true,
            selected_text: true,
            has_reference: true,
            attach_blocked: false,
        });
        assert_eq!(
            rows.iter()
                .filter(|row| row.display_shortcut == 's')
                .count(),
            1
        );
        assert_eq!(
            contextual_key_action(&rows, 's'),
            Some(ContextualAction::Share)
        );
    }
    #[test]
    fn planned_rows_are_visible_but_not_activatable() {
        let rows = contextual_menu_rows(ContextualContext {
            focus: FocusPane::SectionRail,
            section: Section::Chats,
            has_selected_message: false,
            selected_text: false,
            has_reference: false,
            attach_blocked: false,
        });
        assert!(
            rows.iter()
                .any(|row| row.display_label == "Starred" && row.row_style == RowStyle::Disabled)
        );
    }

    #[test]
    fn attach_is_enabled_in_normal_conversation_and_disabled_when_blocked() {
        let normal = contextual_menu_rows(ContextualContext {
            focus: FocusPane::Conversation,
            section: Section::Chats,
            has_selected_message: false,
            selected_text: false,
            has_reference: false,
            attach_blocked: false,
        });
        assert_eq!(
            normal
                .iter()
                .find(|row| row.action_token == ContextualAction::Attach)
                .map(|row| row.row_style),
            Some(RowStyle::Enabled)
        );

        let blocked = contextual_menu_rows(ContextualContext {
            attach_blocked: true,
            ..ContextualContext {
                focus: FocusPane::Conversation,
                section: Section::Chats,
                has_selected_message: false,
                selected_text: false,
                has_reference: false,
                attach_blocked: false,
            }
        });
        assert_eq!(
            blocked
                .iter()
                .find(|row| row.action_token == ContextualAction::Attach)
                .map(|row| row.row_style),
            Some(RowStyle::Disabled)
        );
    }
}
