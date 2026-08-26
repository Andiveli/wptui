use std::{
    ffi::{CStr, CString},
    ops::Range,
    sync::Arc,
};

use strum::{EnumIter, FromRepr};

use crate::abi::{CJID, CMentionRange};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct JID(pub Arc<str>);

impl From<JID> for Arc<str> {
    fn from(jid: JID) -> Self {
        jid.0
    }
}

impl From<String> for JID {
    fn from(jid: String) -> Self {
        JID(jid.into())
    }
}

impl From<&CJID> for JID {
    fn from(cjid: &CJID) -> Self {
        JID(unsafe { CStr::from_ptr(*cjid) }.to_string_lossy().into())
    }
}

impl From<&JID> for CJID {
    fn from(jid: &JID) -> Self {
        CString::new(jid.0.as_ref()).unwrap().into_raw()
    }
}

pub type MessageId = Arc<str>;

#[derive(Clone, Debug)]
pub struct MessageInfo {
    pub id: MessageId,
    pub chat: JID,
    pub sender: JID,
    pub mentions_self: bool,
    pub timestamp: i64,
    pub is_from_me: bool,
    pub quote_id: Option<Arc<str>>,
    pub read_by: u16,
    pub forwarding: ForwardingInfo,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ForwardingInfo {
    pub is_forwarded: bool,
    pub score: u32,
}

#[derive(Clone, Debug, Default, FromRepr)]
#[repr(u8)]
pub enum FileKind {
    #[default]
    Image = 0,
    Video = 1,
    Audio = 2,
    Document = 3,
    Sticker = 4,
}

pub(crate) fn file_kind_discriminant(kind: &FileKind) -> u8 {
    kind.clone() as u8
}

pub type FileId = Arc<str>;

#[derive(Clone, Debug, Default)]
pub struct FileContent {
    pub kind: FileKind,
    pub path: Arc<str>,
    pub file_id: FileId,
    pub caption: Option<Arc<str>>,
}

#[derive(Clone, Debug, EnumIter)]
pub enum MessageContent {
    Text(Arc<str>),
    File(FileContent),
    ViewOnceUnavailable,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub info: MessageInfo,
    pub message: MessageContent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mention {
    pub jid: JID,
    pub numeric_user: Arc<str>,
}
