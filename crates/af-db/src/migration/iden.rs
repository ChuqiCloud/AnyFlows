//! 初始迁移专用的固定表名与列名，避免运行时实体重命名改写历史迁移。

use sea_orm_migration::prelude::DeriveIden;

pub(in crate::migration) mod async_tasks {
    use super::DeriveIden;

    /// `async_tasks` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "async_tasks")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 异步任务持久化迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        TaskKey,
        UserId,
        TokenId,
        GroupId,
        IdempotencyKey,
        Protocol,
        RequestedModel,
        UpstreamModel,
        ChannelId,
        CredentialId,
        CredentialRevision,
        UpstreamTaskId,
        Status,
        ProgressBasisPoints,
        FailureKind,
        Version,
        TerminalAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod async_task_submission_claims {
    use super::DeriveIden;

    /// `async_task_submission_claims` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "async_task_submission_claims")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 异步任务提交 claim 迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        TaskKey,
        UserId,
        TokenId,
        GroupId,
        IdempotencyKey,
        Protocol,
        RequestedModel,
        RequestFingerprint,
        State,
        AttemptKey,
        TargetGroupId,
        UpstreamModel,
        ChannelId,
        CredentialId,
        CredentialRevision,
        UpstreamTaskId,
        BindingFingerprint,
        AttemptTimeoutMillis,
        VideoDurationSeconds,
        VideoResolution,
        Status,
        ProgressBasisPoints,
        FailureKind,
        Version,
        AcceptedAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod async_task_billings {
    use super::DeriveIden;

    /// `async_task_billings` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "async_task_billings")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 异步任务计费迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        TaskKey,
        UserId,
        ReservationKey,
        TargetGroupId,
        State,
        PriceCardVersion,
        BillingResolution,
        GroupRatioMicros,
        GroupModelRatioMicros,
        PeakRatioMicros,
        UpperBound,
        RateMicrousd,
        FallbackQuota,
        ActualQuota,
        ActualDurationSeconds,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod groups {
    use super::DeriveIden;

    /// `groups` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "groups")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// `groups` 初始迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Name,
        DisplayName,
        RatioMicros,
        PeakRatioMicros,
        PeakStart,
        PeakEnd,
        IsExclusive,
        DailyLimit,
        WeeklyLimit,
        MonthlyLimit,
        DailyUsage,
        WeeklyUsage,
        MonthlyUsage,
        DailyWindowStart,
        WeeklyWindowStart,
        MonthlyWindowStart,
        RpmLimit,
        FallbackGroupId,
        Flags,
        CreatedAt,
        UpdatedAt,
        DeletedAt,
    }
}

pub(in crate::migration) mod users {
    use super::DeriveIden;

    /// `users` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "users")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// `users` 初始迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Username,
        Email,
        PasswordHash,
        Role,
        Status,
        DefaultGroupId,
        Quota,
        UsedQuota,
        FrozenQuota,
        RequestCount,
        AffCode,
        InviterId,
        AffQuota,
        AffHistoryQuota,
        TotpSecret,
        RpmLimit,
        Concurrency,
        BalanceAlertThreshold,
        Settings,
        CreatedAt,
        UpdatedAt,
        DeletedAt,
    }
}

pub(in crate::migration) mod oauth_login_providers {
    use super::DeriveIden;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "oauth_login_providers")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Provider,
        Enabled,
        ClientId,
        ClientSecret,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod passkeys {
    use super::DeriveIden;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "passkeys")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        UserId,
        CredentialId,
        CredentialIdDigest,
        Passkey,
        DisplayName,
        CreatedAt,
        LastUsedAt,
        RevokedAt,
        SignCount,
        AnomalyAt,
    }
}

pub(in crate::migration) mod passkey_authentication_challenges {
    use super::DeriveIden;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "passkey_authentication_challenges")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        UserId,
        ChallengeDigest,
        AuthenticationState,
        SessionVersion,
        ExpiresAt,
        ConsumedAt,
        CreatedAt,
    }
}

pub(in crate::migration) mod passkey_registration_challenges {
    use super::DeriveIden;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "passkey_registration_challenges")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        UserId,
        ChallengeDigest,
        RegistrationState,
        ExpiresAt,
        ConsumedAt,
        CreatedAt,
    }
}

pub(in crate::migration) mod user_oauth_identities {
    use super::DeriveIden;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "user_oauth_identities")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        UserId,
        Provider,
        Subject,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod oauth_login_transactions {
    use super::DeriveIden;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "oauth_login_transactions")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Provider,
        StateDigest,
        ExpiresAt,
        ClaimedAt,
        UserId,
        TicketDigest,
        TicketExpiresAt,
        ExchangedAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod channels {
    use super::DeriveIden;

    /// `channels` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "channels")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// `channels` 初始迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Name,
        Type,
        Protocol,
        BaseUrl,
        Status,
        Weight,
        Priority,
        AutoBan,
        ModelMapping,
        ParamOverride,
        HeaderOverride,
        Balance,
        UsedQuota,
        Settings,
        Tag,
        CreatedAt,
        UpdatedAt,
        DeletedAt,
    }
}

pub(in crate::migration) mod credentials {
    use super::DeriveIden;

    /// `credentials` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "credentials")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// `credentials` 初始迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        ChannelId,
        Kind,
        Secret,
        Status,
        MultiKeyMode,
        Priority,
        Weight,
        Concurrency,
        LoadFactorMicros,
        RateMultiplierMicros,
        Schedulable,
        RateLimitedAt,
        RateLimitResetAt,
        OverloadUntil,
        TempUnschedulableUntil,
        TempUnschedulableReason,
        SessionWindowStart,
        SessionWindowEnd,
        ParentId,
        QuotaDimension,
        ProxyId,
        OauthProvider,
        OauthAccountKey,
        OauthProjectId,
        LastUsedAt,
        CreatedAt,
        UpdatedAt,
        DeletedAt,
    }
}

pub(in crate::migration) mod proxies {
    use super::DeriveIden;

    /// `proxies` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "proxies")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 凭据专属出口代理目录的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Name,
        ActiveName,
        Scheme,
        Host,
        Port,
        Username,
        PasswordSecret,
        TrustProxyDns,
        Enabled,
        Version,
        CreatedAt,
        UpdatedAt,
        DeletedAt,
    }
}

pub(in crate::migration) mod tokens {
    use super::DeriveIden;

    /// `tokens` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "tokens")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// `tokens` 初始迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        UserId,
        KeyHash,
        KeyPrefix,
        Name,
        Status,
        GroupId,
        OrganizationId,
        OrganizationMembershipId,
        OrganizationTeamId,
        RemainQuota,
        UnlimitedQuota,
        UsedQuota,
        ExpiredAt,
        ModelLimits,
        AllowIps,
        CrossGroupRetry,
        #[sea_orm(iden = "rate_limit_5h")]
        RateLimit5h,
        #[sea_orm(iden = "rate_limit_1d")]
        RateLimit1d,
        #[sea_orm(iden = "rate_limit_7d")]
        RateLimit7d,
        #[sea_orm(iden = "usage_5h")]
        Usage5h,
        #[sea_orm(iden = "usage_1d")]
        Usage1d,
        #[sea_orm(iden = "usage_7d")]
        Usage7d,
        #[sea_orm(iden = "window_5h_start")]
        Window5hStart,
        #[sea_orm(iden = "window_1d_start")]
        Window1dStart,
        #[sea_orm(iden = "window_7d_start")]
        Window7dStart,
        MaxRequests,
        UsedRequests,
        CreatedAt,
        UpdatedAt,
        DeletedAt,
    }
}

pub(in crate::migration) mod channel_models {
    use super::DeriveIden;

    /// `channel_models` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "channel_models")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// `channel_models` 初始迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        ChannelId,
        Model,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod channel_groups {
    use super::DeriveIden;

    /// `channel_groups` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "channel_groups")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// `channel_groups` 初始迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        ChannelId,
        GroupId,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod abilities {
    use super::DeriveIden;

    /// `abilities` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "abilities")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// `abilities` 初始迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        GroupId,
        Model,
        ChannelId,
        Enabled,
        Priority,
        Weight,
        Tag,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod options {
    use super::DeriveIden;

    /// `options` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "options")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// `options` 初始迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Key,
        Value,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod billing_reservations {
    use super::DeriveIden;

    /// `billing_reservations` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "billing_reservations")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// `billing_reservations` 初始迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        IdempotencyKey,
        UserId,
        TokenId,
        GroupId,
        OrganizationId,
        Status,
        ReservationKind,
        FundingSource,
        ReservedQuota,
        TokenReservedQuota,
        ActualQuota,
        ExpiresAt,
        FinalizedAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod billing_subscription_reservations {
    use super::DeriveIden;

    /// `billing_subscription_reservations` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "billing_subscription_reservations")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 订阅计费预留的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        IdempotencyKey,
        UserSubscriptionId,
        WindowStartedAt,
        WindowEndsAt,
        ReservedQuota,
        SubscriptionActualQuota,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod billing_token_window_reservations {
    use super::DeriveIden;

    /// `billing_token_window_reservations` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "billing_token_window_reservations")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 令牌窗口计费预留的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        IdempotencyKey,
        #[sea_orm(iden = "window_5h_start")]
        Window5hStart,
        #[sea_orm(iden = "window_1d_start")]
        Window1dStart,
        #[sea_orm(iden = "window_7d_start")]
        Window7dStart,
        ReservedQuota,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod billing_group_window_reservations {
    use super::DeriveIden;

    /// `billing_group_window_reservations` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "billing_group_window_reservations")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 分组窗口计费预留的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        IdempotencyKey,
        DailyWindowStart,
        WeeklyWindowStart,
        MonthlyWindowStart,
        ReservedQuota,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod billing_batch_checkpoints {
    use super::DeriveIden;

    /// `billing_batch_checkpoints` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "billing_batch_checkpoints")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 批量落盘检查点迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        WriterKey,
        LastStartSequence,
        LastEndSequence,
        LastEventCount,
        LastFingerprint,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod model_prices {
    use super::DeriveIden;

    /// `model_prices` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "model_prices")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 模型定价目录初始迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Model,
        BillingMode,
        InputPrice,
        OutputPrice,
        CacheReadPrice,
        #[sea_orm(iden = "cache_creation_5m_price")]
        CacheCreation5mPrice,
        #[sea_orm(iden = "cache_creation_1h_price")]
        CacheCreation1hPrice,
        BillingExpression,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod models {
    use super::DeriveIden;

    /// `models` 独立模型商品元数据表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "models")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 模型商品元数据初始迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Model,
        DisplayName,
        Provider,
        Description,
        IconUrl,
        Tags,
        ContextWindow,
        SupportsTextInput,
        SupportsImageInput,
        SupportsAudioInput,
        SupportsVideoInput,
        SupportsTextOutput,
        SupportsImageOutput,
        SupportsAudioOutput,
        SupportsVideoOutput,
        SupportsReasoning,
        SupportsToolCalls,
        Visibility,
        Lifecycle,
        CreatedAt,
        UpdatedAt,
        DeletedAt,
    }
}

pub(in crate::migration) mod model_sync_runs {
    use super::DeriveIden;

    /// `model_sync_runs` 上游模型同步运行审计表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "model_sync_runs")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 模型同步运行的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        PreviewId,
        ChannelId,
        ActorUserId,
        ChannelType,
        Protocol,
        State,
        CandidateCount,
        ExpiresAt,
        AppliedAt,
        CreatedAt,
    }
}

pub(in crate::migration) mod model_sync_items {
    use super::DeriveIden;

    /// `model_sync_items` 上游模型同步候选审计表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "model_sync_items")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 模型同步候选的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        RunId,
        Ordinal,
        CanonicalModel,
        UpstreamModel,
        Relation,
        DisplayNameHint,
        DescriptionHint,
        ContextWindowHint,
        InputTokenLimitHint,
        OutputTokenLimitHint,
        SupportedMethods,
        AppliedModelId,
    }
}

pub(in crate::migration) mod group_model_ratios {
    use super::DeriveIden;

    /// `group_model_ratios` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "group_model_ratios")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 分组间附加倍率迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        SourceGroupId,
        TargetGroupId,
        RatioMicros,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod usage_logs {
    use super::DeriveIden;

    /// `usage_logs` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "usage_logs")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 用量日志最小持久化事实的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        EventId,
        EventType,
        UserId,
        TokenId,
        GroupId,
        OrganizationId,
        OrganizationTeamId,
        BillingMode,
        InputTokens,
        OutputTokens,
        CacheRead,
        #[sea_orm(iden = "cache_creation_5m")]
        CacheCreation5m,
        #[sea_orm(iden = "cache_creation_1h")]
        CacheCreation1h,
        ReasoningTokens,
        AudioInputTokens,
        AudioOutputTokens,
        AudioDurationNanoseconds,
        VideoDurationSeconds,
        VideoResolution,
        RequestId,
        Model,
        Protocol,
        Operation,
        IsStream,
        ReasoningEffort,
        ReasoningBudgetTokens,
        FirstTokenMs,
        DurationMs,
        UsageSource,
        UsageSemantics,
        Quota,
        CreatedAt,
    }
}

pub(in crate::migration) mod playground_shares {
    use super::DeriveIden;

    /// `playground_shares` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "playground_shares")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// Playground 只读分享快照的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        OwnerUserId,
        TokenHash,
        Snapshot,
        CreatedAt,
        ExpiresAt,
        RevokedAt,
    }
}

pub(in crate::migration) mod playground_conversations {
    use super::DeriveIden;

    /// `playground_conversations` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "playground_conversations")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// Playground 私有会话历史的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        ConversationId,
        OwnerUserId,
        Title,
        Models,
        Snapshot,
        Revision,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod registration_rate_limits {
    use super::DeriveIden;

    /// `registration_rate_limits` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "registration_rate_limits")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 公开注册限流迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        IpFingerprint,
        WindowStartedAt,
        Attempts,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod auth_challenges {
    use super::DeriveIden;

    /// `auth_challenges` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "auth_challenges")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 认证挑战迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Purpose,
        SubjectFingerprint,
        SecretDigest,
        TargetUserId,
        Attempts,
        MaxAttempts,
        Version,
        IssuedAt,
        ExpiresAt,
        NextSendAt,
        ConsumedAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod auth_challenge_rate_limits {
    use super::DeriveIden;

    /// `auth_challenge_rate_limits` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "auth_challenge_rate_limits")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 认证挑战发送限流迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Purpose,
        Scope,
        Fingerprint,
        WindowStartedAt,
        Attempts,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod email_settings {
    use super::DeriveIden;

    /// `email_settings` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "email_settings")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 系统 SMTP 邮件设置迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Enabled,
        Host,
        Port,
        TlsMode,
        Username,
        PasswordSecret,
        FromAddress,
        FromName,
        ReplyTo,
        TimeoutSeconds,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod network_settings {
    use super::DeriveIden;

    /// `network_settings` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "network_settings")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 全局出站网络设置迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Mode,
        ProxyHost,
        ProxyPort,
        Username,
        PasswordSecret,
        TrustProxyDns,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod payment_settings {
    use super::DeriveIden;

    /// `payment_settings` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "payment_settings")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 在线支付 Provider 设置迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Initialized,
        StripeEnabled,
        StripePublishableKey,
        StripeSecretKey,
        StripeWebhookSecret,
        StripeSignatureToleranceSeconds,
        EpayEnabled,
        EpayGatewayUrl,
        EpayMerchantId,
        EpayMerchantKey,
        EpayAlipayEnabled,
        EpayWxpayEnabled,
        EpayQuotaPerCny,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod site_settings {
    use super::DeriveIden;

    /// `site_settings` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "site_settings")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 站点身份与公开品牌设置迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        SiteName,
        PublicBaseUrl,
        BrandLogoUrl,
        BrandTagline,
        BrandDescription,
        BalanceDisplayMode,
        BalanceUnitName,
        BalanceUnitSymbol,
        QuotaUnitsPerDisplayUnit,
        BalanceSymbolPosition,
        BalanceFractionDigits,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod authentication_settings {
    use super::DeriveIden;

    /// `authentication_settings` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "authentication_settings")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 密码登录与公开注册设置迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        PasswordLoginEnabled,
        RegistrationEnabled,
        RegistrationDefaultGroupId,
        RegistrationInitialQuota,
        InvitationRebateQuota,
        RegistrationEmailRequired,
        RegistrationRateLimitAttempts,
        RegistrationRateLimitWindowSeconds,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod wallet_ledger_entries {
    use super::DeriveIden;

    /// `wallet_ledger_entries` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "wallet_ledger_entries")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 钱包追加账本迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        EventKey,
        UserId,
        ActorUserId,
        EntryType,
        QuotaDelta,
        BalanceBefore,
        BalanceAfter,
        Reason,
        CreatedAt,
    }
}

pub(in crate::migration) mod topup_orders {
    use super::DeriveIden;

    /// `topup_orders` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "topup_orders")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 充值订单状态机迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        OrderKey,
        UserId,
        OrganizationId,
        Provider,
        PaymentMethod,
        ProviderOrderId,
        TradeNo,
        Status,
        AmountMinor,
        Currency,
        QuotaAmount,
        IdempotencyKey,
        Version,
        ExpiresAt,
        PaidAt,
        ClosedAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod topup_payment_events {
    use super::DeriveIden;

    /// `topup_payment_events` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "topup_payment_events")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 支付 webhook 事件审计迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        EventKey,
        OrderId,
        Provider,
        ProviderEventId,
        TradeNo,
        AmountMinor,
        Currency,
        PaymentMethod,
        EventType,
        SignatureKeyFingerprint,
        PayloadSha256,
        ReceivedAt,
        ProcessedAt,
        CreatedAt,
    }
}

pub(in crate::migration) mod redemption_batches {
    use super::DeriveIden;

    /// `redemption_batches` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "redemption_batches")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 兑换码批次迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        BatchKey,
        Name,
        CreatedByUserId,
        Status,
        QuotaAmount,
        CodeCount,
        Version,
        ExpiresAt,
        DisabledAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod redemption_codes {
    use super::DeriveIden;

    /// `redemption_codes` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "redemption_codes")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 一次性兑换码迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        CodeKey,
        BatchId,
        CodeSha256,
        Status,
        UsedByUserId,
        RedeemedAt,
        CreatedAt,
    }
}

pub(in crate::migration) mod subscription_plans {
    use super::DeriveIden;

    /// `subscription_plans` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "subscription_plans")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 订阅计划迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        PlanKey,
        Name,
        CreatedByUserId,
        Status,
        QuotaAmount,
        Cycle,
        Version,
        DisabledAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod user_subscriptions {
    use super::DeriveIden;

    /// `user_subscriptions` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "user_subscriptions")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 用户订阅迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        SubscriptionKey,
        UserId,
        PlanId,
        PlanVersion,
        Status,
        QuotaAmount,
        QuotaUsed,
        Cycle,
        WindowStartedAt,
        WindowEndsAt,
        Version,
        BoundAt,
        StatusChangedAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod invite_rebate_events {
    use super::DeriveIden;

    /// `invite_rebate_events` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "invite_rebate_events")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 邀请返利到账事件迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        EventKey,
        InviterUserId,
        InviteeUserId,
        QuotaAmount,
        BalanceAfter,
        WalletLedgerEntryId,
        CreditedAt,
        CreatedAt,
    }
}

pub(in crate::migration) mod balance_alert_settings {
    use super::DeriveIden;

    /// `balance_alert_settings` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "balance_alert_settings")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 余额预警全局设置迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Enabled,
        DefaultThresholdQuota,
        ReminderIntervalSeconds,
        SubscriptionAlertEnabled,
        SubscriptionRemainingPercent,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod balance_alert_events {
    use super::DeriveIden;

    /// `balance_alert_events` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "balance_alert_events")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 余额预警投递事件迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        UserId,
        WindowStartedAtEpoch,
        ThresholdQuota,
        ObservedQuota,
        Status,
        AttemptCount,
        NextAttemptAt,
        LeaseExpiresAt,
        LastErrorKind,
        Version,
        SentAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod subscription_balance_alert_events {
    use super::DeriveIden;

    /// `subscription_balance_alert_events` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "subscription_balance_alert_events")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 订阅窗口剩余额度预警事件的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        UserSubscriptionId,
        UserId,
        WindowStartedAt,
        WindowEndsAt,
        ThresholdPercent,
        QuotaAmount,
        ObservedQuotaUsed,
        Status,
        AttemptCount,
        NextAttemptAt,
        LeaseExpiresAt,
        LastErrorKind,
        Version,
        SentAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod scheduler_outbox_events {
    use super::DeriveIden;

    /// `scheduler_outbox_events` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "scheduler_outbox_events")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 调度目录变更 outbox 迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        SubjectKind,
        SubjectId,
        Status,
        AttemptCount,
        NextAttemptAt,
        LeaseExpiresAt,
        PublishedAt,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod routes {
    use super::DeriveIden;

    /// `routes` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "routes")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 智能路由规则迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Name,
        ModelPattern,
        RouteMode,
        Strategy,
        ModelMapping,
        Enabled,
        CreatedAt,
        UpdatedAt,
        DeletedAt,
    }
}

pub(in crate::migration) mod route_channels {
    use super::DeriveIden;

    /// `route_channels` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "route_channels")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 智能路由候选与运行时统计的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        RouteId,
        ChannelId,
        CredentialId,
        Priority,
        Weight,
        Enabled,
        SuccessCount,
        FailCount,
        TotalLatency,
        CooldownLevel,
        CooldownUntil,
        LastSelectedAt,
        LastFailureAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod debug_trace_settings {
    use super::DeriveIden;

    /// `debug_trace_settings` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "debug_trace_settings")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 调试追踪全局设置迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        Enabled,
        SamplePerMillion,
        RetentionHours,
        CaptureHeaders,
        CaptureBodies,
        MaxBodyBytes,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod debug_traces {
    use super::DeriveIden;

    /// `debug_traces` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "debug_traces")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 脱敏请求级调试追踪迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        RequestId,
        UserId,
        TokenId,
        GroupId,
        RequestedModel,
        DownstreamProtocol,
        UpstreamProtocol,
        Operation,
        Outcome,
        SelectedChannelId,
        SelectedCredentialId,
        RoutingElapsedMs,
        AttemptCount,
        DownstreamMethod,
        DownstreamPath,
        DownstreamHeadersJson,
        DownstreamBodyJson,
        CreatedAt,
    }
}

pub(in crate::migration) mod debug_trace_attempts {
    use super::DeriveIden;

    /// `debug_trace_attempts` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "debug_trace_attempts")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 候选尝试调试明细迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        TraceId,
        CandidateIndex,
        ChannelId,
        CredentialId,
        Outcome,
        FailureKind,
        UpstreamStatus,
        RetryDecision,
        ElapsedMs,
        ClientSimulationProfile,
        ClientSimulationResult,
        RequestMethod,
        RequestUrl,
        RequestHeadersJson,
        RequestBodyJson,
        ResponseHeadersJson,
        ResponseBodyJson,
        ResponseStatus,
        ResponseStreamed,
        CreatedAt,
    }
}

pub(in crate::migration) mod debug_trace_snapshots {
    use super::DeriveIden;

    /// `debug_trace_snapshots` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "debug_trace_snapshots")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 字段级密文诊断快照迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        TraceId,
        AttemptId,
        Kind,
        EncryptedPayload,
        CreatedAt,
    }
}

pub(in crate::migration) mod debug_trace_snapshot_access_audits {
    use super::DeriveIden;

    /// `debug_trace_snapshot_access_audits` 的固定表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "debug_trace_snapshot_access_audits")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 敏感快照读取审计迁移的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        TraceId,
        ActorUserId,
        Scope,
        Outcome,
        CreatedAt,
    }
}

pub(in crate::migration) mod organizations {
    use super::DeriveIden;

    /// `organizations` 企业主体表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "organizations")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 企业主体的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        OrganizationKey,
        Name,
        Slug,
        Status,
        CreatedByUserId,
        Version,
        CreatedAt,
        UpdatedAt,
        ClosedAt,
    }
}

pub(in crate::migration) mod organization_teams {
    use super::DeriveIden;

    /// `organization_teams` 企业团队表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "organization_teams")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 企业团队的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        OrganizationId,
        Name,
        Description,
        Status,
        Version,
        CreatedAt,
        UpdatedAt,
        DeletedAt,
    }
}

pub(in crate::migration) mod organization_audit_logs {
    use super::DeriveIden;

    /// `organization_audit_logs` 企业管理审计表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "organization_audit_logs")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 企业管理审计的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        OrganizationId,
        ActorUserId,
        ActorRole,
        PermissionCode,
        Operation,
        TargetType,
        TargetIdentifier,
        Outcome,
        PlatformProxy,
        PlatformReason,
        RequestId,
        ClientFingerprint,
        ChangeSummary,
        CreatedAt,
    }
}

pub(in crate::migration) mod organization_memberships {
    use super::DeriveIden;

    /// `organization_memberships` 企业成员关系表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "organization_memberships")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 企业成员关系的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        OrganizationId,
        UserId,
        Role,
        TeamId,
        Status,
        JoinedAt,
        SuspendedAt,
        RemovedAt,
        InvitedByUserId,
        Version,
    }
}

pub(in crate::migration) mod organization_entitlements {
    use super::DeriveIden;

    /// 企业商业授权表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "organization_entitlements")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 企业商业授权的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        OrganizationId,
        TeamManagementEnabled,
        SeatLimit,
        TeamLimit,
        OrganizationKeyLimit,
        AuditRetentionDays,
        SsoEnabled,
        CustomRolesEnabled,
        ValidFrom,
        ValidUntil,
        GraceUntil,
        Source,
        ExternalReference,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod organization_invitations {
    use super::DeriveIden;

    /// `organization_invitations` 企业邀请表标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "organization_invitations")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    /// 企业邀请的固定列标识。
    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        InvitationKey,
        OrganizationId,
        EmailFingerprint,
        EmailSecret,
        TokenDigest,
        Role,
        TeamId,
        Status,
        InvitedByUserId,
        AcceptedByUserId,
        BatchKey,
        BatchDigest,
        BatchSize,
        Version,
        CreatedAt,
        ExpiresAt,
        FinalizedAt,
    }
}

pub(in crate::migration) mod organization_wallets {
    use super::DeriveIden;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "organization_wallets")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        OrganizationId,
        Quota,
        UsedQuota,
        FrozenQuota,
        BillingProtected,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod organization_budget_policies {
    use super::DeriveIden;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "organization_budget_policies")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        OrganizationId,
        ScopeKind,
        TeamId,
        MembershipId,
        PeriodKind,
        FixedPeriodSeconds,
        QuotaLimit,
        EffectiveFrom,
        EffectiveUntil,
        CreatedByUserId,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod organization_budget_windows {
    use super::DeriveIden;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "organization_budget_windows")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        PolicyId,
        OrganizationId,
        ScopeKind,
        TeamId,
        MembershipId,
        PolicyVersion,
        LimitSnapshot,
        UsedQuota,
        FrozenQuota,
        WindowStartsAt,
        WindowEndsAt,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod billing_organization_reservations {
    use super::DeriveIden;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "billing_organization_reservations")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        IdempotencyKey,
        OrganizationId,
        MembershipId,
        TeamId,
        UserId,
        TokenId,
        GroupId,
        ReservationKind,
        Status,
        ReservedQuota,
        TokenReservedQuota,
        ActualQuota,
        OrganizationPolicyId,
        OrganizationWindowId,
        OrganizationPolicyVersion,
        OrganizationWindowVersion,
        OrganizationReservedQuota,
        TeamPolicyId,
        TeamWindowId,
        TeamPolicyVersion,
        TeamWindowVersion,
        TeamReservedQuota,
        MemberPolicyId,
        MemberWindowId,
        MemberPolicyVersion,
        MemberWindowVersion,
        MemberReservedQuota,
        ExpiresAt,
        FinalizedAt,
        CreatedAt,
        UpdatedAt,
    }
}

pub(in crate::migration) mod organization_wallet_ledger_entries {
    use super::DeriveIden;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Table {
        #[sea_orm(iden = "organization_wallet_ledger_entries")]
        Name,
    }

    pub(in crate::migration) use Table::Name as Entity;

    #[derive(Clone, Copy, DeriveIden)]
    pub(in crate::migration) enum Column {
        Id,
        OrganizationId,
        EventKey,
        ActorUserId,
        EntryType,
        QuotaDelta,
        BalanceBefore,
        BalanceAfter,
        Source,
        Reason,
        CreatedAt,
    }
}
