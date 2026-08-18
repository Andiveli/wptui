use super::actions::AppAction;

/// Actions allowed while a contact's status view is focused (read-only).
/// Chat-specific actions are rejected so nothing ever targets the
/// `status@broadcast` chat from the status pane.
pub(crate) fn status_view_allows(action: &AppAction) -> bool {
    matches!(
        action,
        AppAction::Quit
            | AppAction::Logout
            | AppAction::ToggleLogs
            | AppAction::ToggleSectionRail
            | AppAction::ToggleChatList
            | AppAction::FocusNext
            | AppAction::FocusPrevious
            | AppAction::SelectNext
            | AppAction::SelectPrevious
            | AppAction::JumpTop
            | AppAction::JumpBottom
            | AppAction::HalfPageDown
            | AppAction::HalfPageUp
            | AppAction::CopyMessage
            | AppAction::ReplyMessage
            | AppAction::ReactMessage
            | AppAction::DownloadMessage
            | AppAction::ViewMessage
            | AppAction::ViewerNext
            | AppAction::ViewerPrevious
            | AppAction::ViewerZoomIn
            | AppAction::ViewerZoomOut
            | AppAction::CloseAttachmentViewer
            | AppAction::ViewerOpenExternal
            | AppAction::CloseStatusPane
    )
}

#[cfg(test)]
mod tests {
    use super::status_view_allows;
    use crate::app::actions::AppAction;

    #[test]
    fn status_view_allows_status_interactions_and_media_actions() {
        for allowed in [
            AppAction::Quit,
            AppAction::Logout,
            AppAction::ToggleLogs,
            AppAction::ToggleSectionRail,
            AppAction::ToggleChatList,
            AppAction::FocusNext,
            AppAction::FocusPrevious,
            AppAction::SelectNext,
            AppAction::SelectPrevious,
            AppAction::JumpTop,
            AppAction::JumpBottom,
            AppAction::HalfPageDown,
            AppAction::HalfPageUp,
            AppAction::CopyMessage,
            AppAction::ReactMessage,
            AppAction::ReplyMessage,
            AppAction::DownloadMessage,
            AppAction::ViewMessage,
            AppAction::ViewerNext,
            AppAction::ViewerPrevious,
            AppAction::ViewerZoomIn,
            AppAction::ViewerZoomOut,
            AppAction::CloseAttachmentViewer,
            AppAction::ViewerOpenExternal,
            AppAction::CloseStatusPane,
        ] {
            assert!(status_view_allows(&allowed), "{allowed:?} must be allowed");
        }
    }

    #[test]
    fn status_view_rejects_chat_only_actions() {
        for blocked in [
            AppAction::OpenChat,
            AppAction::OpenMessage,
            AppAction::InsertMode,
            AppAction::DeleteMessage,
            AppAction::EditMessage,
            AppAction::OpenMessageMenu,
            AppAction::GoToReference,
        ] {
            assert!(!status_view_allows(&blocked), "{blocked:?} must be blocked");
        }
    }
}
