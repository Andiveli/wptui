use std::sync::Arc;

use crate::app::MessagePushNamePort;
use whatsrust as wr;

pub struct WhatsRustMessagePushName;

impl MessagePushNamePort for WhatsRustMessagePushName {
    fn lookup_push_name(&self, message_id: &wr::MessageId) -> Option<Arc<str>> {
        wr::message_push_name(message_id)
    }
}
