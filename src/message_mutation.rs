use crate::app::actions::{MessageEditor, MessageForwarder, MessageReactor, MessageRevoker};
use whatsrust as wr;

pub struct WhatsAppMessageEditor;

impl MessageEditor for WhatsAppMessageEditor {
    fn edit_message(
        &self,
        chat: &wr::JID,
        message_id: &wr::MessageId,
        replacement: &str,
    ) -> Result<(), wr::MessageActionFailed> {
        wr::edit_message(chat, message_id, replacement)
    }
}

pub struct WhatsAppMessageReactor;

impl MessageReactor for WhatsAppMessageReactor {
    fn react_to_message(
        &self,
        chat: &wr::JID,
        sender: &wr::JID,
        message_id: &wr::MessageId,
        reaction: &str,
    ) -> Result<(), wr::MessageActionFailed> {
        wr::react_to_message(chat, sender, message_id, reaction)
    }

    fn react_to_message_in_chat(
        &self,
        target: &wr::JID,
        destination: &wr::JID,
        sender: &wr::JID,
        message_id: &wr::MessageId,
        reaction: &str,
    ) -> Result<(), wr::MessageActionFailed> {
        wr::react_to_message_in_chat(target, destination, sender, message_id, reaction)
    }
}

pub struct WhatsAppMessageForwarder;

impl MessageForwarder for WhatsAppMessageForwarder {
    fn forward_message(&self, source: &wr::Message, destinations: &[wr::JID]) -> wr::ForwardReport {
        wr::forward_message(source, destinations)
    }
}

pub struct WhatsAppMessageRevoker;

impl MessageRevoker for WhatsAppMessageRevoker {
    fn revoke_message(
        &self,
        chat: &wr::JID,
        sender: &wr::JID,
        message_id: &wr::MessageId,
    ) -> Result<(), wr::MessageActionFailed> {
        wr::revoke_message(chat, sender, message_id)
    }
}
