#!/usr/bin/env bash
set -Eeuo pipefail

# 真实 ClickHouse 只验证 HTTP 查询与 JSONEachRow 解码，主库仍由测试自身创建的 SQLite 提供。
readonly TEST_NAME="configured_clickhouse_supplies_analytics_while_database_supplies_channels"

if [[ -z "${ANYFLOWS_CLICKHOUSE_INTEGRATION_URL:-}" ]]; then
  echo "缺少 ANYFLOWS_CLICKHOUSE_INTEGRATION_URL" >&2
  exit 2
fi

cargo test --locked --package af-server --test admin_dashboard_chain "${TEST_NAME}" -- --exact --nocapture
