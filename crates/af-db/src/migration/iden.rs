//! 初始迁移专用的固定表名与列名，避免运行时实体重命名改写历史迁移。

use sea_orm_migration::prelude::DeriveIden;

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
        Settings,
        CreatedAt,
        UpdatedAt,
        DeletedAt,
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
        Status,
        ReservedQuota,
        TokenReservedQuota,
        ActualQuota,
        ExpiresAt,
        FinalizedAt,
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
        RegistrationEmailRequired,
        RegistrationRateLimitAttempts,
        RegistrationRateLimitWindowSeconds,
        Version,
        CreatedAt,
        UpdatedAt,
    }
}
