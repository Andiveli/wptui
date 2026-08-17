use std::collections::HashMap;
use whatsrust as wr;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChatTimelineState {
    pub pending_new_messages: usize,
    pub last_read_message: Option<wr::MessageId>,
    pub last_read_at: Option<i64>,
}

pub type Timeline = HashMap<wr::JID, ChatTimelineState>;
