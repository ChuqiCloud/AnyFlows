//! Request-based service levels from persisted terminal outcomes.

use std::{future::Future, pin::Pin};

use crate::{AdminDashboardReadError, AdminDashboardStorageError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceLevelDimension {
    Model,
    Channel,
}

#[derive(Clone, Debug)]
pub struct ServiceLevelQuery {
    pub dimension: ServiceLevelDimension,
    pub search: String,
    pub page: u32,
    pub page_size: u32,
    pub failures_first: bool,
}

impl ServiceLevelQuery {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.page > 0
            && self.page <= 10_000
            && (1..=20).contains(&self.page_size)
            && self.search.chars().count() <= 128
    }
}

#[derive(Clone, Debug)]
pub struct ServiceLevelPoint {
    pub period_start: i64,
    pub successful_request_count: i64,
    pub failed_request_count: i64,
    pub unknown_request_count: i64,
}

#[derive(Clone, Debug)]
pub struct ServiceLevelRow {
    pub key: String,
    pub name: String,
    pub request_count: i64,
    pub successful_request_count: i64,
    pub failed_request_count: i64,
    pub unknown_request_count: i64,
    pub average_duration_ms: Option<i64>,
    pub hourly: Vec<ServiceLevelPoint>,
}

#[derive(Clone, Debug)]
pub struct ServiceLevelReport {
    pub period_start: i64,
    pub period_end: i64,
    pub total: i64,
    pub unattributed_request_count: i64,
    pub items: Vec<ServiceLevelRow>,
}

pub type ServiceLevelReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ServiceLevelReport, AdminDashboardReadError>> + Send + 'a>>;

pub type ServiceLevelStorageFuture<'a> = Pin<
    Box<dyn Future<Output = Result<ServiceLevelReport, AdminDashboardStorageError>> + Send + 'a>,
>;

pub trait ServiceLevelStorage: Send + Sync {
    fn report(
        &self,
        period_start: i64,
        period_end: i64,
        query: ServiceLevelQuery,
    ) -> ServiceLevelStorageFuture<'_>;
}
