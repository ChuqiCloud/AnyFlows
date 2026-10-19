use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProxyCleanupTarget {
    Pools,
    Nodes,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProxyCleanupMode {
    Selected,
    Failed,
    Unused,
}
pub struct ProxyCleanupRequest {
    pub target: ProxyCleanupTarget,
    pub mode: ProxyCleanupMode,
    pub pool_id: Option<ProxyId>,
    pub revision: Option<i64>,
    pub ids: Vec<ProxyId>,
    pub after_id: Option<ProxyId>,
    pub preview: bool,
}
#[derive(Default)]
pub struct ProxyCleanupResult {
    pub eligible_ids: Vec<i64>,
    pub deleted_ids: Vec<i64>,
    pub protected_ids: Vec<i64>,
    pub skipped_ids: Vec<i64>,
    pub more_available: bool,
    pub next_after_id: Option<i64>,
}
impl ProxyPoolRepository {
    /// 预览与执行共享筛选；执行时重新检查引用和版本，绝不自动解除绑定。
    pub async fn cleanup(&self, request: ProxyCleanupRequest) -> Result<ProxyCleanupResult> {
        self.run(async {
            let ProxyCleanupRequest {
                target,
                mode,
                pool_id,
                revision,
                ids,
                after_id,
                preview,
            } = request;
            if ids.len() > 256
                || (mode == ProxyCleanupMode::Selected && ids.is_empty())
                || (mode == ProxyCleanupMode::Selected && after_id.is_some())
                || (target == ProxyCleanupTarget::Pools
                    && (mode != ProxyCleanupMode::Selected || pool_id.is_some()))
                || (mode == ProxyCleanupMode::Failed && pool_id.is_none())
                || (mode == ProxyCleanupMode::Unused && pool_id.is_some())
                || (!preview && ids.is_empty())
                || (pool_id.is_none() && revision.is_some())
                || revision.is_some_and(|value| value <= 0 || value == i64::MAX)
            {
                return Err(ProxyPoolRepositoryError::InvalidInput);
            }
            let tx = self.pool.connection().begin().await.map_err(internal)?;
            let now = TimeDateTimeWithTimeZone::now_utc();
            let _pool_guards = if !preview {
                // 成员替换先写池行；统一按 ID 加锁，避免清理和订阅更新交错。
                Some(
                    pools::Entity::find()
                        .order_by_asc(pools::Column::Id)
                        .lock_exclusive()
                        .all(&tx)
                        .await
                        .map_err(internal)?,
                )
            } else {
                None
            };
            if let Some(id) = pool_id {
                let pool = pools::Entity::find_by_id(id.get())
                    .one(&tx)
                    .await
                    .map_err(internal)?
                    .ok_or(ProxyPoolRepositoryError::NotFound)?;
                if Some(pool.revision) != revision || pool.work_until > now.unix_timestamp() {
                    return Err(ProxyPoolRepositoryError::Conflict);
                }
                if !preview {
                    let guarded = pools::Entity::update_many()
                        .filter(pools::Column::Id.eq(id.get()))
                        .filter(pools::Column::Revision.eq(pool.revision))
                        .filter(pools::Column::WorkUntil.lte(now.unix_timestamp()))
                        .col_expr(pools::Column::Revision, Expr::value(pool.revision + 1))
                        .exec(&tx)
                        .await
                        .map_err(internal)?;
                    if guarded.rows_affected != 1 {
                        return Err(ProxyPoolRepositoryError::Conflict);
                    }
                }
            }
            let selected = ids.into_iter().map(|id| id.get()).collect::<BTreeSet<_>>();
            let mut query = proxies::Entity::find().filter(proxies::Column::DeletedAt.is_null());
            if !selected.is_empty() {
                query = query.filter(proxies::Column::Id.is_in(selected.iter().copied()));
            }
            if let Some(after_id) = after_id {
                query = query.filter(proxies::Column::Id.gt(after_id.get()));
            }
            if target == ProxyCleanupTarget::Pools {
                query = query.filter(
                    proxies::Column::Id.in_subquery(
                        pools::Entity::find()
                            .select_only()
                            .column(pools::Column::Id)
                            .into_query(),
                    ),
                );
            } else {
                query = query.filter(
                    proxies::Column::Id.not_in_subquery(
                        pools::Entity::find()
                            .select_only()
                            .column(pools::Column::Id)
                            .into_query(),
                    ),
                );
                if let Some(id) = pool_id {
                    let mut members = nodes::Entity::find()
                        .select_only()
                        .column(nodes::Column::ProxyId)
                        .filter(nodes::Column::PoolId.eq(id.get()))
                        .filter(nodes::Column::Present.eq(true));
                    if mode == ProxyCleanupMode::Failed {
                        // 只清理明确失败；尚未检测或配置变更后待重测的节点不计入。
                        members = members
                            .filter(nodes::Column::Healthy.eq(false))
                            .filter(nodes::Column::CheckedAt.is_not_null())
                            .filter(nodes::Column::LatencyMs.is_null());
                    }
                    query = query.filter(proxies::Column::Id.in_subquery(members.into_query()));
                }
                if mode == ProxyCleanupMode::Unused {
                    query = query
                        .filter(
                            proxies::Column::Id.not_in_subquery(
                                nodes::Entity::find()
                                    .select_only()
                                    .column(nodes::Column::ProxyId)
                                    .filter(nodes::Column::Present.eq(true))
                                    .into_query(),
                            ),
                        )
                        .filter(
                            proxies::Column::Id.not_in_subquery(
                                credentials::Entity::find()
                                    .select_only()
                                    .column(credentials::Column::ProxyId)
                                    .filter(credentials::Column::ProxyId.is_not_null())
                                    .filter(credentials::Column::DeletedAt.is_null())
                                    .into_query(),
                            ),
                        );
                }
            }
            let mut candidates_query = query.clone().order_by_asc(proxies::Column::Id).limit(257);
            if !preview {
                candidates_query = candidates_query.lock_exclusive();
            }
            let mut candidates = candidates_query.all(&tx).await.map_err(internal)?;
            let mut result = ProxyCleanupResult {
                more_available: candidates.len() > 256,
                ..Default::default()
            };
            candidates.truncate(256);
            if result.more_available {
                result.next_after_id = candidates.last().map(|proxy| proxy.id);
            }
            let found = candidates.iter().map(|p| p.id).collect::<BTreeSet<_>>();
            result.skipped_ids = selected.difference(&found).copied().collect();
            for proxy in candidates {
                let pool_guard = if target == ProxyCleanupTarget::Pools && !preview {
                    Some(
                        pools::Entity::find_by_id(proxy.id)
                            .lock_exclusive()
                            .one(&tx)
                            .await
                            .map_err(internal)?,
                    )
                } else {
                    None
                };
                // 锁定目录行后再检查引用；与凭据绑定写入共用同一目录保护边界。
                if !preview
                    && (proxies::Entity::find_by_id(proxy.id)
                        .filter(proxies::Column::DeletedAt.is_null())
                        .lock_exclusive()
                        .one(&tx)
                        .await
                        .map_err(internal)?
                        .is_none()
                        || query
                            .clone()
                            .filter(proxies::Column::Id.eq(proxy.id))
                            .lock_exclusive()
                            .one(&tx)
                            .await
                            .map_err(internal)?
                            .is_none())
                {
                    result.skipped_ids.push(proxy.id);
                    continue;
                }
                let referenced = credentials::Entity::find()
                    .filter(credentials::Column::ProxyId.eq(proxy.id))
                    .filter(credentials::Column::DeletedAt.is_null())
                    .lock_exclusive()
                    .one(&tx)
                    .await
                    .map_err(internal)?
                    .is_some();
                let protected = if target == ProxyCleanupTarget::Pools {
                    let pool = if let Some(guard) = pool_guard {
                        guard
                    } else {
                        pools::Entity::find_by_id(proxy.id)
                            .one(&tx)
                            .await
                            .map_err(internal)?
                    };
                    referenced
                        || pool.is_none_or(|p| p.work_until > now.unix_timestamp())
                        || bindings::Entity::find()
                            .filter(bindings::Column::PoolId.eq(proxy.id))
                            .lock_exclusive()
                            .one(&tx)
                            .await
                            .map_err(internal)?
                            .is_some()
                } else {
                    let mut members = nodes::Entity::find()
                        .filter(nodes::Column::ProxyId.eq(proxy.id))
                        .filter(nodes::Column::Present.eq(true));
                    if let Some(id) = pool_id {
                        members = members.filter(nodes::Column::PoolId.ne(id.get()));
                    }
                    referenced
                        || members
                            .lock_exclusive()
                            .one(&tx)
                            .await
                            .map_err(internal)?
                            .is_some()
                };
                if protected {
                    result.protected_ids.push(proxy.id);
                    continue;
                }
                result.eligible_ids.push(proxy.id);
                if preview {
                    continue;
                }
                if target == ProxyCleanupTarget::Pools {
                    nodes::Entity::delete_many()
                        .filter(nodes::Column::PoolId.eq(proxy.id))
                        .exec(&tx)
                        .await
                        .map_err(internal)?;
                    pools::Entity::delete_by_id(proxy.id)
                        .exec(&tx)
                        .await
                        .map_err(internal)?;
                } else {
                    nodes::Entity::update_many()
                        .filter(nodes::Column::ProxyId.eq(proxy.id))
                        .col_expr(nodes::Column::Present, Expr::value(false))
                        .col_expr(nodes::Column::Healthy, Expr::value(false))
                        .exec(&tx)
                        .await
                        .map_err(internal)?;
                }
                proxies::Entity::update_many()
                    .filter(proxies::Column::Id.eq(proxy.id))
                    .filter(proxies::Column::DeletedAt.is_null())
                    .col_expr(
                        proxies::Column::ActiveName,
                        Expr::value(Option::<String>::None),
                    )
                    .col_expr(proxies::Column::DeletedAt, Expr::value(now))
                    .col_expr(proxies::Column::UpdatedAt, Expr::value(now))
                    .exec(&tx)
                    .await
                    .map_err(internal)?;
                result.deleted_ids.push(proxy.id);
            }
            tx.commit().await.map_err(internal)?;
            Ok(result)
        })
        .await
    }
}

#[cfg(test)]
mod tests;
