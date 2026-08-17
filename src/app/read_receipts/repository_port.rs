use super::{ReceiptCandidate, ReceiptKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryError {
    Busy,
    Unavailable,
    Schema,
}

pub trait PendingReceiptRepository: Send {
    fn load(&self) -> Result<Vec<ReceiptCandidate>, RepositoryError>;
    fn save(&self, candidate: &ReceiptCandidate) -> Result<(), RepositoryError>;
    fn was_sent(&self, key: &ReceiptKey) -> Result<bool, RepositoryError>;
    fn complete_success(&self, key: &ReceiptKey) -> Result<(), RepositoryError>;
    fn reject(&self, key: &ReceiptKey) -> Result<(), RepositoryError>;
}
