use std::sync::Arc;

use whatsrust as wr;

pub trait MessagePushNamePort {
    fn lookup_push_name(&self, message_id: &wr::MessageId) -> Option<Arc<str>>;
}
