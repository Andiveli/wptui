use super::{ReadReceiptPort, ReceiptCandidate, ReceiptSendStatus};

pub struct WhatsAppAdapter;

impl ReadReceiptPort for WhatsAppAdapter {
    fn send(&self, candidate: &ReceiptCandidate) -> ReceiptSendStatus {
        let chat = whatsrust::JID::from(candidate.chat.clone());
        let sender = whatsrust::JID::from(candidate.sender.clone());
        let id = whatsrust::MessageId::from(candidate.message_id.clone());
        match whatsrust::mark_as_read(&id, &chat, &sender) {
            Ok(()) => ReceiptSendStatus::Success,
            Err(whatsrust::MarkAsReadError::Disconnected) => ReceiptSendStatus::Disconnected,
            Err(whatsrust::MarkAsReadError::Permanent) => ReceiptSendStatus::Permanent,
            Err(whatsrust::MarkAsReadError::Transient) => ReceiptSendStatus::Transient,
        }
    }
}
