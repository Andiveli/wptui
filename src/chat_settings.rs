use crate::app::ChatSettingsQueryPort;
use whatsrust as wr;

pub struct WhatsRustChatSettingsQuery;

impl ChatSettingsQueryPort for WhatsRustChatSettingsQuery {
    fn get_chat_settings(&self, jid: &wr::JID) -> wr::ChatSettings {
        wr::get_chat_settings(jid)
    }
}
