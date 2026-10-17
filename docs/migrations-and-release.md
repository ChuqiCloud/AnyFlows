# 升级与备份

升级会替换程序，并在启动时执行待处理的数据库迁移。升级前阅读目标版本说明，保留当前版本安装包，并安排可恢复的备份。

## 备份哪些内容

| 部署方式 | 至少保留 |
| --- | --- |
| systemd 默认 SQLite | `/etc/anyflows/anyflows.env`、整个 `/var/lib/anyflows` 和当前版本归档 |
| PostgreSQL / MySQL | 数据库一致性备份、应用配置、每个节点的计费 WAL、当前版本归档 |
| Docker Compose | `.env`、数据库备份、Redis 数据及 WAL 命名卷、上一版镜像或构建来源 |

配置中的凭据加密密钥和标识必须与数据库一起保留。外部前端模板也需备份。

单机 SQLite 可在维护窗口停止服务后备份配置和整个数据目录，避免只复制仍在写入的数据库文件：

```bash
sudo systemctl stop anyflows
sudo install -d -m 0700 /var/backups/anyflows
sudo sh -c 'umask 077; tar -czf "/var/backups/anyflows/backup-$(date +%Y%m%d-%H%M%S).tar.gz" -C / etc/anyflows var/lib/anyflows'
sudo systemctl start anyflows
```

备份包含密钥和业务数据，应另存到受控备份位置并验证可恢复性。使用外部数据库时，上面的文件备份不能代替数据库备份；需按数据库官方工具取得一致性快照。计费 WAL 是实例恢复日志，不是数据库备份。

## 二进制升级

下载新归档及对应校验文件，校验并解压；命令与 [首次安装](deployment-and-configuration.md#2-下载并校验) 相同。在新版本目录执行：

```bash
sudo ./install.sh upgrade
sudo ./install.sh status
curl --fail http://127.0.0.1:8080/readyz
sudo journalctl -u anyflows -n 100 --no-pager
```

安装器保留已有配置和上一份二进制，重启后循环检查 `/readyz`。新版本无法就绪时，存在上一份二进制则尝试恢复。也可在归档目录执行：

```bash
sudo ./install.sh rollback
```

回滚只恢复程序，不撤销数据库迁移、配置或 WAL。新版本已改变数据库且旧版不兼容时，应使用前向修复版本，或按完整恢复方案还原兼容的数据和配置，不能仅切回旧程序。

## 企业版切换到社区版

企业版直接替换为社区版后，社区迁移器会识别拆分前遗留在 `seaql_migrations` 中的企业迁移版本，并继续执行社区迁移。兼容范围仅限已确认的企业迁移编号；其他未知迁移仍会让启动失败。

降级不会删除企业表、企业业务数据或迁移历史，也不会把企业功能转换成社区功能。切换前应备份数据库和配置，并确认企业数据的保留策略；如需彻底移除企业数据，应另行制定并审核数据清理方案，不能通过删除迁移记录来处理。兼容细节见 [公共核心与扩展边界](repository-boundary.md#数据库迁移与访问)。

## 多节点和容器

多节点升级先摘除待升级节点、等待长请求结束，再停止服务。先由一个新版本节点完成迁移并检查就绪，再启动其余节点。没有确认跨版本兼容时使用维护窗口，不直接混跑版本。配置步骤见 [多节点部署](multi-node-deployment.md)。

容器部署先完成备份，获取目标源码版本后执行 `docker compose --env-file .env up -d --build`。失败时保留数据卷，检查日志和迁移状态，再决定恢复镜像或部署修复。不要用删除数据卷的方式排障。

## 升级后确认

确认服务就绪、能够登录、渠道凭据可读取，并执行一次小额度模型调用，核对日志和用量。`/healthz` 仅表示存活，`/readyz` 用于依赖就绪检查，二者不能代替实际调用。

迁移历史由应用管理，不要手工删除记录或改表绕过失败。企业扩展的迁移接管机制见 [扩展边界](repository-boundary.md#数据库迁移与访问)。
