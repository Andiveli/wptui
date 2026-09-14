use std::{fmt, sync::Arc};

use whatsrust as wr;

#[derive(Clone, Debug)]
pub struct StoreStatusCursor {
    pub contact: wr::JID,
    pub timestamp: i64,
}

pub trait StatusCursorPort {
    fn load(&self) -> Result<Vec<(wr::JID, i64)>, StatusCursorError>;
    fn store(&self, command: StoreStatusCursor) -> Result<(), StatusCursorError>;
}

#[derive(Clone, Debug)]
pub struct StatusCursorError(pub Arc<str>);

impl fmt::Display for StatusCursorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for StatusCursorError {}
