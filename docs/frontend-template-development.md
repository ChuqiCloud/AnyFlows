# 前端模板

AnyFlows 内置 Classic 和 Next。管理员可在模板设置中预览、搜索和切换模板。内置模板使用布局示意图，外部模板的预览由作者提供。

外部模板是完整的静态前端产物。后端扫描并校验资源，将其读入内存；启用后从站点根路径提供。模板的 JavaScript 在浏览器中执行，应使用可信来源的模板。

## 构建和目录

模板应能独立构建，无需连接后端或数据库。将构建产物放入运行目录下的 `public/templates/<id>/`：

```text
public/templates/dark-console/
├── template.json
├── index.html
├── preview.png
└── assets/
    ├── app-<hash>.js
    └── app-<hash>.css
```

将 `dist/` 内的文件直接放入模板目录，不包含源码、依赖或环境密钥。`template.json` 示例：

```json
{
  "schema_version": 1,
  "id": "dark-console",
  "name": "Dark Console",
  "version": "1.0.0",
  "api_contract": "0.2",
  "entry": "index.html",
  "description": "深色控制台",
  "author": "Template Team",
  "preview": "preview.png"
}
```

`api_contract` 声明模板兼容的后端契约，发布前需验证兼容性。

| 字段或内容 | 要求 |
| --- | --- |
| `schema_version` | 固定为 1，拒绝未知字段 |
| `id` | 与目录名一致，最多 64 个 ASCII 小写字母、数字、`-` 或 `_` |
| 保留 ID | 外部模板不能使用 `embedded`、`embedded-next` |
| `name` | 非空，最多 160 字节，无控制字符 |
| `version`、`api_contract` | 非空，各最多 64 字节，无控制字符 |
| `entry` | 固定为 `index.html`，且文件存在 |
| `description` / `author` | 可选，分别最多 1000 / 160 字节，无控制字符 |
| `preview` | 可选的本地相对 PNG、JPEG、WebP，最多 2 MiB，签名与格式一致 |
| 资源 | 单文件最多 8 MiB，整套最多 64 MiB，最多 4096 个文件 |
| `template.json` | 最多 64 KiB |

模板拒绝目录穿越、符号链接、特殊文件、远程预览地址及 HTML/SVG 预览。预览文件无效时不能启用模板。资源须完整，缺失文件返回 404。

## 扫描与切换

默认模板目录为运行目录下的 `public/templates`。`AF_SERVER__FRONTEND_TEMPLATE_DIRECTORY` 可指定运行目录内的其他相对路径。

按仓库提供的 systemd 配置部署时，模板放在 `/var/lib/anyflows/public/templates/<id>/`，并授予 `anyflows` 用户读取权限。Docker 默认工作目录为 `/app`，模板挂载到 `/app/public/templates`。多节点需使用相同模板。

复制完成后在管理后台重新扫描、预览并启用。扫描生成资源快照，修改磁盘文件后需再次扫描。模板选择保存到数据库，重启后恢复。

`embedded` 和空值选择 Classic，`embedded-next` 选择 Next。已选外部模板损坏或缺失时，重新扫描后回退到 Classic；修复后可重新扫描并选择。切换后页面刷新。

模板资源使用 `/assets/...` 等根路径。`/api/...`、`/v1`、`/healthz`、`/readyz`、`/metrics` 及其子路径由后端处理，不回退到模板 HTML；`/api` 为前端接口说明页。

## 接口和缓存

接口以对应后端版本的 [OpenAPI 文件](../web/openapi/openapi.json) 为准。在线说明位于 `/api` 页面；服务不提供 `/openapi.json`、Swagger UI 或 ReDoc 路由。

同源请求使用 `/api/...`、`/v1/...`。使用仓库中的前端工程构建跨域版本时，设置 `VITE_API_BASE_URL`，并在后端配置对应 CORS Origin。构建变量不得包含生产密钥。

`assets/` 资源使用一年 immutable 缓存，HTML 等入口重新验证。更新 JS/CSS 时使用新的内容哈希文件名，并更新入口引用。

内置前端开发与客户端生成见 [贡献指南](../CONTRIBUTING.md)，构建时源码模块见 [前端扩展](frontend-extensions.md)。

## 发布前检查

- 登录、退出、模型列表、控制台和 Playground。
- 控制台深链接刷新、移动端布局、请求失败和权限不足状态。
- 流式响应、资源缓存和 404。
- 模板扫描、预览、切换、重启恢复和损坏回退。
