use serde::{Deserialize, Serialize};

/// 用户路由组在当前用户授权分组之间选择候选的策略。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRouteStrategy {
    PriceFirst,
    StableFirst,
    Ordered,
    RoundRobin,
    Weighted,
}

impl UserRouteStrategy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PriceFirst => "price_first",
            Self::StableFirst => "stable_first",
            Self::Ordered => "ordered",
            Self::RoundRobin => "round_robin",
            Self::Weighted => "weighted",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "price_first" => Some(Self::PriceFirst),
            "stable_first" => Some(Self::StableFirst),
            "ordered" => Some(Self::Ordered),
            "round_robin" => Some(Self::RoundRobin),
            "weighted" => Some(Self::Weighted),
            _ => None,
        }
    }
}

/// 每个用户允许保留的路由组数量。
pub const MAX_USER_ROUTES: usize = 32;
/// 单条路由允许的实体分组数量及最大尝试次数。
pub const MAX_USER_ROUTE_MEMBERS: usize = 16;
/// 单个成员的最大流量权重。
pub const MAX_USER_ROUTE_WEIGHT: u32 = 10_000;
