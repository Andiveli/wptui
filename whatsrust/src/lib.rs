//! Public facade for the WhatsRust bridge.
//! Implementation and focused tests live in owning modules.
//! Explicit reexports keep the public API boundary auditable.

use std::ffi::CString;

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
use abi::*;
pub use abi::{LogoutStatus, ReceiptKind};
pub use actions::{edit_message, react_to_message, react_to_message_in_chat, revoke_message};
pub use callbacks::CallbackTranslator;
pub use events::set_event_handler;
pub use lifecycle::{connect, disconnect, logout, new_client, pair_phone};
pub use media::{download_file, get_community_profile_picture, get_profile_picture};
pub use message_send::{
    ForwardFailure, ForwardReport, TextSendResult, forward_message, send_message, send_text_message,
};
pub(crate) use models::file_kind_discriminant;
pub use models::{
    ChatSettings, CommunitiesError, CommunityInfo, Contact, DownloadFailed, Event, FileContent,
    FileId, FileKind, ForwardingInfo, GroupInfo, GroupInfoError, GroupParticipant, JID,
    LogoutError, Mention, Message, MessageActionFailed, MessageActionKind, MessageContent,
    MessageId, MessageInfo, PresenceUpdate, ProfilePicture, ProfilePictureAvailability,
    ProfilePictureError,
};
pub use presence::{SubscribePresenceResult, drain_raw_presence_diagnostics, subscribe_presence};
pub use queries::{
    get_chat_settings, get_communities, get_contacts, get_group_info, get_group_participants,
    resolve_dm_chat,
};
pub use read_sync::{MarkAsReadError, mark_as_read, sync_chat_read};
pub use registrations::{
    set_log_handler, set_message_handler, set_optimistic_text_sent_handler, set_presence_handler,
};

pub use caches::{forward_source, message_mention_ranges, message_push_name};
pub use caches::{
    remove_forward_source, store_forward_source, store_message_mention_ranges,
    store_message_push_name,
};

pub const VIEW_ONCE_UNAVAILABLE_DESCRIPTION: &str =
    "View-once media is unavailable here. View it on your phone.";

impl CallbackTranslator<CJID> for JID {
    unsafe fn to_rust(from: CJID) -> Self {
        (&from).into()
    }
}

impl CallbackTranslator<i64> for i64 {
    unsafe fn to_rust(value: i64) -> Self {
        value
    }
}
