# 升级与备份

程序启动时自动执行待处理的数据库迁移。升级前阅读目标版本说明，备份配置和数据，并保留当前版本安装包。

## 备份范围

| 部署方式 | 至少保留 |
| --- | --- |
| systemd + SQLite | `/etc/anyflows/anyflows.env`、整个 `/var/lib/anyflows` 和当前版本归档 |
| PostgreSQL / MySQL | 数据库一致性备份、应用配置、每个节点的计费 WAL、当前版本归档 |
| Docker Compose | `.env`、数据库备份、Redis 数据及 WAL 命名卷、上一版镜像或构建来源 |

数据库备份须同时保留凭据加密密钥和标识；使用外部前端模板时，一并备份模板文件。

单机 SQLite 可在维护窗口停止服务后备份配置和整个数据目录，避免只复制仍在写入的数据库文件：

```bash
sudo systemctl stop anyflows
sudo install -d -m 0700 /var/backups/anyflows
sudo sh -c 'umask 077; tar -czf "/var/backups/anyflows/backup-$(date +%Y%m%d-%H%M%S).tar.gz" -C / etc/anyflows var/lib/anyflows'
sudo systemctl start anyflows
```

备份包含密钥和业务数据，应保存到受控位置并验证可恢复性。外部数据库需使用相应数据库工具取得一致性快照；上述文件备份不包含外部数据库。计费 WAL 仅用于实例恢复，不能替代数据库备份。

## 二进制升级

下载目标版本归档及校验文件，按 [首次安装](deployment-and-configuration.md#2-下载并校验) 校验、解压。在新版本目录停止服务，保留当前程序，再安装新程序和服务单元：

```bash
sudo systemctl stop anyflows
sudo install -o root -g root -m 0755 /usr/local/libexec/anyflows/anyflows /usr/local/libexec/anyflows/anyflows.previous
sudo install -o root -g root -m 0644 /etc/systemd/system/anyflows.service /etc/systemd/system/anyflows.service.previous
sudo install -o root -g root -m 0755 ./anyflows /usr/local/libexec/anyflows/anyflows
sudo install -o root -g root -m 0644 ./anyflows.service /etc/systemd/system/anyflows.service
sudo systemctl daemon-reload
sudo systemctl start anyflows
sudo systemctl --no-pager status anyflows
curl --fail http://127.0.0.1:8080/readyz
sudo journalctl -u anyflows -n 100 --no-pager
```

保留 `/etc/anyflows/anyflows.env` 和数据目录，不使用新归档中的示例配置覆盖已有配置。迁移期间 `/readyz` 可能暂不可用，应结合日志等待完成。

新版本无法启动时，先检查日志。确认旧版本与当前数据库、配置兼容后，可恢复上一份程序：

```bash
sudo systemctl stop anyflows
sudo install -o root -g root -m 0755 /usr/local/libexec/anyflows/anyflows.previous /usr/local/libexec/anyflows/anyflows
sudo install -o root -g root -m 0644 /etc/systemd/system/anyflows.service.previous /etc/systemd/system/anyflows.service
sudo systemctl daemon-reload
sudo systemctl start anyflows
curl --fail http://127.0.0.1:8080/readyz
```

程序回退不撤销数据库迁移、配置或 WAL。数据库变更与旧版不兼容时，部署修复版本，或按完整恢复方案还原兼容的程序、服务单元、配置和数据。

## 多节点和容器

多节点升级时，将待升级节点从负载均衡摘除，等待长请求结束后停止服务。由一个新版本节点完成迁移并检查就绪，再启动其余节点。未确认跨版本兼容时使用维护窗口，详见 [多节点部署](multi-node-deployment.md)。

容器部署完成备份并检出目标源码版本后，执行 `docker compose --env-file .env up -d --build`。失败时保留数据卷，检查日志和迁移状态，确认兼容后恢复旧镜像或部署修复版本。

## 升级后确认

确认 `/readyz` 返回成功、版本接口符合预期、能够登录且渠道凭据可读取。执行一次小额度模型调用，核对日志和用量。`/healthz` 仅表示进程存活，不能代替就绪检查和实际调用。

迁移历史由应用管理。迁移失败时保留记录和数据库，按日志定位原因。
