use std::error::Error;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurgeExpiredStatuses {
    pub now: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurgedExpiredStatuses {
    pub media_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusRetentionError(pub Arc<str>);

impl fmt::Display for StatusRetentionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for StatusRetentionError {}

pub trait StatusRetentionPort {
    fn purge_expired_statuses(
        &self,
        command: PurgeExpiredStatuses,
    ) -> Result<PurgedExpiredStatuses, StatusRetentionError>;
}
