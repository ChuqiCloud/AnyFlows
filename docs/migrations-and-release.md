# 迁移与发布说明

本文说明 AnyFlows 版本升级、数据库迁移和失败回滚之间的关系。它适用于 Docker Compose 和 Linux systemd 两种部署方式。

## 发布物

版本号来自工作区 `Cargo.toml` 的 `package.version`，正式标签格式为 `v<版本号>`。Linux amd64 归档包含：

- 内嵌前端资源的 `anyflows` 单二进制。
- `README.md`、`.env.example` 和 `CHANGELOG.md`。
- systemd `install.sh`、`anyflows.service` 和记录标签及提交的 `VERSION`。
- 与归档对应的 `.sha256` 和发布汇总中的 `SHA256SUMS`。

下载后先校验 SHA-256，再在隔离环境检查 `/healthz` 和 `/readyz`。不要直接执行来源不明或校验失败的归档。

## 数据库迁移

应用启动时执行待处理迁移，迁移历史由应用维护。升级前必须备份 PostgreSQL、MySQL 或 SQLite 数据，并备份计费 WAL；多实例部署应安排单一迁移窗口，避免多个旧版本同时启动。

迁移按前向兼容策略设计，发布脚本不会自动撤销已执行的迁移。禁止手工删除迁移历史或直接修改生产表结构来绕过失败。迁移失败时先保留日志和数据库快照，再在隔离环境修复或准备前向修复版本。

### 企业迁移历史接管

企业迁移从公共仓库移出时，已有数据库中的企业版本仍记录在
`seaql_migrations`。发行版必须为扩展注册表创建明确的
`MigrationHistoryAdoption` 声明，并通过
`MigratorExtension::with_legacy_history` 启动迁移。系统只接管扩展注册表中存在、且
已经在旧表执行过的版本，保留原始 `applied_at`，随后执行其余待处理迁移。新安装没有
旧表记录时会直接执行扩展迁移。不要把公共迁移表中的全部版本批量复制到扩展表，也不要
手工删除旧记录；接管完成后仍保留公共迁移历史，便于公共核心继续升级。

## systemd 升级与回滚

在新归档目录执行 `sudo ./install.sh upgrade`。脚本会原子替换二进制、保留上一份二进制，并在 `127.0.0.1:8080/readyz` 上等待最多 30 秒。新版本未就绪时，存在上一份二进制则自动恢复；也可执行 `sudo ./install.sh rollback` 交换当前与上一份二进制。

systemd 的回滚只恢复程序文件，不会降级数据库迁移、配置文件、计费 WAL 或 Redis 数据。若新版本已经成功执行不可逆数据库迁移，不能只切回旧二进制；应恢复兼容的数据库快照，或部署包含前向兼容修复的版本，并重新执行就绪检查。

## Docker Compose 升级与回滚

升级前备份 `.env` 和命名卷。使用新代码构建并检查服务状态：

```bash
docker compose --env-file .env up -d --build
docker compose --env-file .env ps
curl --fail http://127.0.0.1:8080/readyz
```

应用未就绪时先停止新容器并保留命名卷，再恢复上一份已校验的镜像。不要执行 `docker compose down -v` 排障，因为该命令会删除数据库、Redis 和计费 WAL。Compose 与 systemd 一样不会自动降级数据库迁移；发生迁移兼容性问题时必须使用数据库快照或前向修复版本。

## 发布后检查

1. 检查 `VERSION` 中的标签和提交是否与发布记录一致。
2. 检查 `/healthz`、`/readyz`、应用日志和迁移结果。
3. 执行一条受控管理请求和一条低额度网关请求，确认鉴权、数据库和计费链路正常。
4. 检查 WAL、数据库和 Redis 的磁盘/内存监控；确认反向代理仍只转发到回环地址。
5. 记录发布提交、归档摘要、迁移结果和回滚决定，不把密钥或请求正文写入记录。
