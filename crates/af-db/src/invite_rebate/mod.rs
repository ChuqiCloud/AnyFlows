mod repository;
#[cfg(test)]
mod tests;
mod types;

pub use repository::InviteRebateRepository;
pub(crate) use repository::grant_in_transaction;
pub use types::{
    InviteRebateGrant, InviteRebateGrantOutcome, InviteRebateInputError, InviteRebateRecord,
    InviteRebateRejection, InviteRebateRepositoryConfigError, InviteRebateRepositoryError,
};
