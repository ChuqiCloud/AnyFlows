# 前端模板

AnyFlows 同时内置 Classic 和 Next。管理员可在模板设置中预览、搜索、分页查看并切换模板，不需要更换后端或数据库。内置预览是布局示意图，外部模板预览由模板作者提供。

外部模板是完整的静态前端产物。服务扫描、校验后将资源读入内存，启用后从站点根路径提供；服务器不执行模板中的脚本。浏览器会执行模板的 JavaScript，因此只安装可信来源的模板。

## 构建和目录

模板需能独立构建，不依赖运行中的后端或数据库。将构建输出的内容放入运行目录下的 `public/templates/<id>/`：

```text
public/templates/dark-console/
├── template.json
├── index.html
├── preview.png
└── assets/
    ├── app-<hash>.js
    └── app-<hash>.css
```

复制 `dist/` 内的文件，不要再套一层 `dist/`，也不要复制源码、依赖或环境密钥。元数据示例：

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

`api_contract` 应声明模板实际兼容的后端契约，示例值不代替发布前验证。

| 字段或内容 | 要求 |
| --- | --- |
| `schema_version` | 固定为 1，拒绝未知字段 |
| `id` | 与目录名一致，最多 64 个 ASCII 小写字母、数字、`-` 或 `_` |
| 保留 ID | 外部模板不能使用 `embedded`、`embedded-next` |
| `name`、`version`、`api_contract` | 非空短文本，无控制字符 |
| `entry` | 固定为 `index.html`，且文件存在 |
| `description` / `author` | 可选，分别最多 1000 / 160 字节，无控制字符 |
| `preview` | 可选的本地相对 PNG、JPEG、WebP，最多 2 MiB，签名与格式一致 |
| 资源 | 单文件最多 8 MiB，整套最多 64 MiB，最多 4096 个文件 |
| `template.json` | 最多 64 KiB |

目录穿越、符号链接、特殊文件、远程预览地址和 HTML/SVG 预览均被拒绝。声明的预览文件无效时，模板不能启用。资源必须完整，缺失文件返回 404，不从内置前端补齐。

## 扫描与切换

默认模板路径是相对运行目录的 `public/templates`，可通过 `AF_SERVER__FRONTEND_TEMPLATE_DIRECTORY` 覆盖，仍须使用运行目录内的相对路径。

systemd 的运行目录是 `/var/lib/anyflows`，默认外部模板因此放在 `/var/lib/anyflows/public/templates/<id>/`，并确保 `anyflows` 用户可读。Docker 需把目录挂载到容器工作目录对应位置。多节点需分发相同模板。

复制完成后在后台重新扫描，再预览并启用。扫描会生成资源内存快照，直接修改磁盘文件不会自动刷新；更新后再次扫描。选择保存到数据库，重启后恢复。

`embedded` 选择 Classic，`embedded-next` 选择 Next；空值保持 Classic 回退语义。已选外部模板损坏或消失时，重新扫描会回退 Classic；修复后重新扫描和选择。切换后页面刷新，不中断进行中的 API 请求。

模板资源使用 `/assets/...` 等根路径。`/api`、`/v1`、`/healthz`、`/readyz`、`/metrics` 始终由后端处理，不回退到模板 HTML。

## 接口和缓存

以相同后端版本的 [OpenAPI 文件](../web/openapi/openapi.json) 为准。服务没有直接提供 `/openapi.json`、Swagger UI 或 ReDoc 路由；在线查阅可使用 `/api` 页面。

同源请求使用 `/api/...`、`/v1/...`。使用本仓前端工程构建跨域版本时，设置 `VITE_API_BASE_URL`，并在后端配置对应 CORS Origin。不要把生产密钥写入构建变量。

`assets/` 资源使用一年 immutable 缓存，HTML 等入口重新验证。每次发布改变 JS/CSS 的内容哈希文件名并更新入口引用，避免覆盖同名资源后浏览器继续读旧内容。

开发内置前端和生成客户端的方法见 [贡献指南](../CONTRIBUTING.md)。源码扩展模块与静态模板是不同机制，见 [前端扩展](frontend-extensions.md)。

## 发布前检查

确认登录、退出、模型列表、控制台和 Playground 可用；直接刷新控制台深链接仍可加载。检查移动端布局、请求失败、权限不足、流式响应、资源缓存与 404。最后验证模板扫描、切换、重启恢复和损坏回退。