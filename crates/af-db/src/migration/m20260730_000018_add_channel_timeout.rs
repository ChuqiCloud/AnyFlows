use af_domain::{MAX_CHANNEL_TIMEOUT_SECS, MIN_CHANNEL_TIMEOUT_SECS};
use sea_orm_migration::prelude::*;

use super::iden::channels;

#[derive(DeriveIden)]
enum ChannelTimeoutColumn {
    TimeoutSecs,
}

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let minimum = <i32 as std::convert::TryFrom<u64>>::try_from(MIN_CHANNEL_TIMEOUT_SECS)
            .expect("渠道超时最小值必须能写入整数列");
        let maximum = <i32 as std::convert::TryFrom<u64>>::try_from(MAX_CHANNEL_TIMEOUT_SECS)
            .expect("渠道超时最大值必须能写入整数列");
        manager
            .alter_table(
                Table::alter()
                    .table(channels::Entity)
                    .add_column(
                        ColumnDef::new(ChannelTimeoutColumn::TimeoutSecs)
                            .integer()
                            .check(
                                Expr::col(ChannelTimeoutColumn::TimeoutSecs)
                                    .between(minimum, maximum),
                            ),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(channels::Entity)
                    .drop_column(ChannelTimeoutColumn::TimeoutSecs)
                    .to_owned(),
            )
            .await
    }
}
