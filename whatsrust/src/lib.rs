//! Public facade for the WhatsRust bridge.
//! Implementation and focused tests live in owning modules.
//! Explicit reexports keep the public API boundary auditable.

#[macro_use]
mod callbacks;
mod abi;
mod actions;
mod caches;
mod events;
#[cfg(test)]
mod facade_tests;
mod incoming;
mod lifecycle;
mod media;
mod message_send;
mod models;
mod presence;
mod queries;
mod read_sync;
mod registrations;
pub use abi::{LogoutStatus, ReceiptKind};
pub use actions::{edit_message, react_to_message, react_to_message_in_chat, revoke_message};
pub use callbacks::CallbackTranslator;
pub use events::set_event_handler;
pub use lifecycle::{connect, disconnect, logout, new_client, pair_phone};
pub use media::{download_file, get_community_profile_picture, get_profile_picture};
pub use message_send::{
    ForwardFailure, ForwardReport, OutboundSendFailure, TextSendResult, forward_message,
    send_message, send_outbound_message, send_text_message,
};
pub(crate) use models::file_kind_discriminant;
pub use models::{
    ChatSettings, CommunitiesError, CommunityInfo, Contact, DownloadFailed, Event, FileContent,
    FileId, FileKind, ForwardingInfo, GroupInfo, GroupInfoError, GroupParticipant, JID,
    LogoutError, Mention, Message, MessageActionFailed, MessageActionKind, MessageContent,
    MessageId, MessageInfo, PresenceUpdate, ProfilePicture, ProfilePictureAvailability,
    ProfilePictureError, VIEW_ONCE_UNAVAILABLE_DESCRIPTION,
};
pub use presence::{SubscribePresenceResult, drain_raw_presence_diagnostics, subscribe_presence};
pub use queries::{
    get_chat_settings, get_communities, get_contacts, get_group_info, get_group_participants,
    resolve_dm_chat,
};
pub use read_sync::{MarkAsReadError, ReadSyncWorker, mark_as_read, sync_chat_read};
pub use registrations::{
    set_log_handler, set_message_handler, set_optimistic_text_sent_handler, set_presence_handler,
};

/// The bounded result of publishing to the user's WhatsApp status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusSendResult {
    Sent,
    UnsupportedContent,
    InvalidContent,
    ClientUnavailable,
    MediaPreparationFailed,
    SendFailed,
}

unsafe extern "C" {
    fn C_SendStatusMessage(message_type: u8, content: *const std::ffi::c_void) -> u8;
}

fn status_content_allowed(content: &MessageContent) -> Result<(), StatusSendResult> {
    match content {
        MessageContent::Text(text) if text.contains('\0') => {
            return Err(StatusSendResult::InvalidContent);
        }
        MessageContent::Text(_) => {}
        MessageContent::File(file) if matches!(file.kind, FileKind::Image | FileKind::Video) => {
            if file.path.contains('\0')
                || file.file_id.contains('\0')
                || file
                    .caption
                    .as_ref()
                    .is_some_and(|caption| caption.contains('\0'))
            {
                return Err(StatusSendResult::InvalidContent);
            }
        }
        _ => return Err(StatusSendResult::UnsupportedContent),
    }
    Ok(())
}

fn status_send_result_from_code(code: u8) -> StatusSendResult {
    match code {
        0 => StatusSendResult::Sent,
        1 => StatusSendResult::UnsupportedContent,
        2 => StatusSendResult::InvalidContent,
        3 => StatusSendResult::ClientUnavailable,
        4 => StatusSendResult::MediaPreparationFailed,
        _ => StatusSendResult::SendFailed,
    }
}

pub fn send_status(content: &MessageContent) -> StatusSendResult {
    if let Err(result) = status_content_allowed(content) {
        return result;
    }
    let (message_type, pointer, _holder) = message_send::build_content_for_ffi(content, &[]);
    status_send_result_from_code(unsafe { C_SendStatusMessage(message_type, pointer) })
}

#[cfg(test)]
mod status_send_tests {
    use super::*;

    #[test]
    fn bridge_codes_are_bounded() {
        for (code, expected) in [
            (0, StatusSendResult::Sent),
            (1, StatusSendResult::UnsupportedContent),
            (2, StatusSendResult::InvalidContent),
            (3, StatusSendResult::ClientUnavailable),
            (4, StatusSendResult::MediaPreparationFailed),
            (5, StatusSendResult::SendFailed),
            (255, StatusSendResult::SendFailed),
        ] {
            assert_eq!(status_send_result_from_code(code), expected);
        }
    }

    #[test]
    fn status_accepts_text_image_video_without_nul_and_rejects_other_kinds() {
        assert_eq!(
            status_content_allowed(&MessageContent::Text("status".into())),
            Ok(())
        );
        assert_eq!(
            status_content_allowed(&MessageContent::Text("bad\0status".into())),
            Err(StatusSendResult::InvalidContent)
        );
        for (kind, expected) in [
            (FileKind::Image, Ok(())),
            (FileKind::Video, Ok(())),
            (FileKind::Audio, Err(StatusSendResult::UnsupportedContent)),
        ] {
            let file = MessageContent::File(FileContent {
                kind,
                path: "status.png".into(),
                file_id: "".into(),
                caption: None,
            });
            assert_eq!(status_content_allowed(&file), expected);
        }
    }
}

pub use caches::{forward_source, message_mention_ranges, message_push_name};
pub use caches::{
    remove_forward_source, store_forward_source, store_message_mention_ranges,
    store_message_push_name,
};
