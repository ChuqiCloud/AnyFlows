# 前端模板与扩展

AnyFlows 内置 Classic 和 Next。静态模板可在运行时切换，源码扩展随前端构建装配。接口定义及客户端生成见 [贡献指南](../CONTRIBUTING.md)。

## 静态模板

模板是可独立构建的完整静态站点。后端校验资源并读入内存，启用后从站点根路径提供；JavaScript 在浏览器执行，应使用可信来源。

### 目录与元数据

将 `dist/` 内的构建产物直接放入运行目录下的 `public/templates/<id>/`，不包含源码、依赖或环境密钥：

```text
public/templates/dark-console/
├── template.json
├── index.html
├── preview.png
└── assets/
    ├── app-<hash>.js
    └── app-<hash>.css
```

`template.json` 示例：

```json
{
  "schema_version": 1,
  "id": "dark-console",
  "name": "Dark Console",
  "version": "1.0.0",
  "api_contract": "0.2",
  "entry": "index.html",
  "preview": "preview.png"
}
```

| 字段或内容 | 要求 |
| --- | --- |
| `schema_version` | 固定为 1，不接受未知字段 |
| `id` | 与目录名一致，最多 64 个 ASCII 小写字母、数字、`-` 或 `_`；保留 `embedded`、`embedded-next` |
| `name` | 非空，最多 160 字节，无控制字符 |
| `version`、`api_contract` | 非空，各最多 64 字节，无控制字符；契约声明需通过兼容验证 |
| `entry` | 固定为 `index.html`，文件须存在 |
| `description`、`author` | 可选，分别最多 1000、160 字节，无控制字符 |
| `preview` | 可选的本地相对 PNG、JPEG、WebP，最多 2 MiB，签名与格式一致 |
| 资源 | 单文件最多 8 MiB，合计最多 64 MiB、4096 个文件；元数据最多 64 KiB |

目录穿越、符号链接、特殊文件、远程预览和 HTML/SVG 预览会被拒绝。声明的预览无效时不能启用模板；缺失资源返回 404。

### 安装与切换

`AF_SERVER__FRONTEND_TEMPLATE_DIRECTORY` 可指定运行目录内的其他相对路径。systemd 默认模板位置是 `/var/lib/anyflows/public/templates`，需授予 `anyflows` 用户读取权限；Docker 默认挂载到 `/app/public/templates`。多节点使用相同模板。

复制后在后台重新扫描、预览并启用。扫描创建内存快照，更新磁盘文件后需再次扫描。选择保存到数据库，重启后恢复；切换后页面刷新。

`embedded` 和空值选择 Classic，`embedded-next` 选择 Next。外部模板缺失或损坏时，重新扫描后回退 Classic；修复后可重新选择。

### 接口与缓存

接口以对应版本的 [OpenAPI](../web/openapi/openapi.json) 为准，在线说明位于 `/api`。服务不提供 `/openapi.json`、Swagger UI 或 ReDoc。

资源使用 `/assets/...` 等根路径。`/api/...`、`/v1`、`/healthz`、`/readyz`、`/metrics` 及子路径由后端处理，不回退到模板 HTML。跨域构建使用 `VITE_API_BASE_URL`，后端需配置对应 CORS Origin；构建变量不得包含生产密钥。

`assets/` 使用一年 immutable 缓存，HTML 等入口重新验证。更新 JS/CSS 时使用新的内容哈希文件名并更新入口引用。

## 构建扩展

Classic 和 Next 从各自的 `src/extensions/modules/*/index.ts` 加载 `ConsoleExtension`。模块声明唯一 ID、翻译资源及懒加载路由，接口见 [`registry.ts`](../web/src/extensions/registry.ts) 和 [`model.ts`](../web/src/extensions/model.ts)。修改后需重新构建对应前端。

### 路由与权限

- 路径位于 `/console/extensions/<name>` 且唯一，声明 `user` 或 `admin` 角色及至少一个必需能力。
- `GET /api/extensions` 返回所需全部能力后显示导航；直接访问需处理加载、失败及能力缺失。后端独立校验身份、归属和权限。
- `navigation` 可配置分组 ID、翻译标题、图标和排序。宿主按角色及能力过滤后分组，未声明时使用默认分组。
- 用户路由可声明唯一的 `entryPoint: 'account-verification'`，作为实名认证完成后的入口；能力缺失时隐藏。

Classic 使用 hash 路由，Next 使用浏览器路由。扩展复用宿主的初始化、会话、API 客户端和查询缓存，页面分别适配两套主题。

### 登录扩展

`loginMethods` 声明模块内唯一的方法 ID、翻译标签、图标和懒加载表单。宿主提供返回操作及注册开放状态。回调方法实现 `exchangeTicket`，返回 `LoginResponse` 后由宿主建立会话：

```text
/#/oauth/callback?method=<module>:<method>&ticket=<ticket>
```

Next 同时支持该 fragment 和浏览器回调路由。宿主清除回调参数后兑换 ticket，其他有效的单值参数传给对应方法；未知或重复方法被拒绝。扩展复用宿主会话存储，ticket、受保护响应和凭据不得写入查询缓存或浏览器持久化存储。

## 发布前检查

- 登录、退出、模型列表、控制台、Playground 及流式响应。
- 深链接刷新、移动端布局、空数据、请求失败和权限不足状态。
- 模板扫描、预览、切换、重启恢复、损坏回退、资源缓存及 404。
- 扩展角色、能力过滤、直接访问、登录回调及双前端共享契约。
