use std::ffi::CString;

use super::*;
use crate::abi::{C_EditMessage, C_ReactToMessage, C_RevokeMessage};

pub(crate) fn reaction_to_ffi(
    target_jid: &JID,
    destination_jid: &JID,
    sender_jid: &JID,
    message_id: &MessageId,
    reaction: &str,
) -> Result<(CString, CString, CString, CString, CString), MessageActionFailed> {
    Ok((
        CString::new(target_jid.0.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(destination_jid.0.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(sender_jid.0.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(message_id.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(reaction).map_err(|_| MessageActionFailed)?,
    ))
}

pub fn react_to_message(
    chat_jid: &JID,
    sender_jid: &JID,
    message_id: &MessageId,
    reaction: &str,
) -> Result<(), MessageActionFailed> {
    react_to_message_in_chat(chat_jid, chat_jid, sender_jid, message_id, reaction)
}

pub fn react_to_message_in_chat(
    target_jid: &JID,
    destination_jid: &JID,
    sender_jid: &JID,
    message_id: &MessageId,
    reaction: &str,
) -> Result<(), MessageActionFailed> {
    let (target, destination, sender, id, reaction) = reaction_to_ffi(
        target_jid,
        destination_jid,
        sender_jid,
        message_id,
        reaction,
    )?;
    let result = unsafe {
        C_ReactToMessage(
            target.as_ptr(),
            destination.as_ptr(),
            sender.as_ptr(),
            id.as_ptr(),
            reaction.as_ptr(),
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(MessageActionFailed)
    }
}

pub(crate) fn edit_to_ffi(
    chat_jid: &JID,
    message_id: &MessageId,
    replacement: &str,
) -> Result<(CString, CString, CString), MessageActionFailed> {
    if replacement.trim().is_empty() {
        return Err(MessageActionFailed);
    }
    Ok((
        CString::new(chat_jid.0.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(message_id.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(replacement).map_err(|_| MessageActionFailed)?,
    ))
}

pub fn edit_message(
    chat_jid: &JID,
    message_id: &MessageId,
    replacement: &str,
) -> Result<(), MessageActionFailed> {
    let (chat, id, replacement) = edit_to_ffi(chat_jid, message_id, replacement)?;
    let result = unsafe { C_EditMessage(chat.as_ptr(), id.as_ptr(), replacement.as_ptr()) };
    if result == 0 {
        Ok(())
    } else {
        Err(MessageActionFailed)
    }
}

pub(crate) fn revoke_to_ffi(
    chat_jid: &JID,
    sender_jid: &JID,
    message_id: &MessageId,
) -> Result<(CString, CString, CString), MessageActionFailed> {
    Ok((
        CString::new(chat_jid.0.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(sender_jid.0.as_ref()).map_err(|_| MessageActionFailed)?,
        CString::new(message_id.as_ref()).map_err(|_| MessageActionFailed)?,
    ))
}

pub fn revoke_message(
    chat_jid: &JID,
    sender_jid: &JID,
    message_id: &MessageId,
) -> Result<(), MessageActionFailed> {
    let (chat, sender, id) = revoke_to_ffi(chat_jid, sender_jid, message_id)?;
    let result = unsafe { C_RevokeMessage(chat.as_ptr(), sender.as_ptr(), id.as_ptr()) };
    if result == 0 {
        Ok(())
    } else {
        Err(MessageActionFailed)
    }
}
