mod repository;
#[cfg(test)]
mod tests;
mod types;

pub use repository::InviteRebateRepository;
pub use types::{
    InviteRebateGrant, InviteRebateGrantOutcome, InviteRebateInputError, InviteRebateRecord,
    InviteRebateRejection, InviteRebateRepositoryConfigError, InviteRebateRepositoryError,
};
