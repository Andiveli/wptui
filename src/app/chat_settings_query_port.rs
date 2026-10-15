use whatsrust as wr;

pub trait ChatSettingsQueryPort {
    fn get_chat_settings(&self, jid: &wr::JID) -> wr::ChatSettings;
}
