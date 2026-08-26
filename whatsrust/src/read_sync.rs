use std::ffi::{CString, c_char};

use crate::{
    abi::*,
    models::{JID, MessageId},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkAsReadError {
    Disconnected,
    Transient,
    Permanent,
}

fn with_borrowed_mark_read_args<T>(
    msg_id: &MessageId,
    chat_jid: &JID,
    sender_jid: &JID,
    send: impl FnOnce(*const c_char, *const c_char, *const c_char) -> T,
) -> Result<T, MarkAsReadError> {
    let msg_id_c = CString::new(msg_id.as_ref()).map_err(|_| MarkAsReadError::Permanent)?;
    let chat_jid_c = CString::new(chat_jid.0.as_ref()).map_err(|_| MarkAsReadError::Permanent)?;
    let sender_jid_c =
        CString::new(sender_jid.0.as_ref()).map_err(|_| MarkAsReadError::Permanent)?;
    Ok(send(
        msg_id_c.as_ptr(),
        chat_jid_c.as_ptr(),
        sender_jid_c.as_ptr(),
    ))
}

pub fn mark_as_read(
    msg_id: &MessageId,
    chat_jid: &JID,
    sender_jid: &JID,
) -> Result<(), MarkAsReadError> {
    let result =
        with_borrowed_mark_read_args(msg_id, chat_jid, sender_jid, |id, chat, sender| unsafe {
            C_MarkAsRead(id, chat, sender)
        })?;
    match result {
        0 => Ok(()),
        1 => Err(MarkAsReadError::Disconnected),
        3 => Err(MarkAsReadError::Permanent),
        _ => Err(MarkAsReadError::Transient),
    }
}

pub fn sync_chat_read(
    chat_jid: &JID,
    message_id: &MessageId,
    timestamp: i64,
    from_me: bool,
    participant_jid: Option<&JID>,
) {
    let chat = chat_jid.0.to_string();
    let message = message_id.to_string();
    let participant = participant_jid.map(|jid| jid.0.to_string());
    std::thread::spawn(move || {
        let chat_c = match CString::new(chat) {
            Ok(value) => value,
            Err(_) => {
                log::warn!("chat read sync skipped: invalid chat JID");
                return;
            }
        };
        let message_c = match CString::new(message) {
            Ok(value) => value,
            Err(_) => {
                log::warn!("chat read sync skipped: invalid message ID");
                return;
            }
        };
        let participant_c = participant.and_then(|value| CString::new(value).ok());
        let participant_ptr = participant_c
            .as_ref()
            .map_or(std::ptr::null(), |value| value.as_ptr());
        let result = unsafe {
            C_MarkChatReadSync(
                chat_c.as_ptr(),
                message_c.as_ptr(),
                timestamp,
                from_me,
                participant_ptr,
            )
        };
        if result != 0 {
            log::warn!("chat read sync failed with bridge status {result}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_ffi_arguments_can_be_reused_without_owned_pointer_leaks() {
        let id: MessageId = "message".into();
        let chat = JID::from("chat@s.whatsapp.net".to_owned());
        let sender = JID::from("sender@s.whatsapp.net".to_owned());
        for _ in 0..1_000 {
            with_borrowed_mark_read_args(&id, &chat, &sender, |id, chat, sender| {
                assert!(!id.is_null() && !chat.is_null() && !sender.is_null());
            })
            .unwrap();
        }
    }
}
