# AnyFlows 前端模板开发规范

本文档适用于部署在 AnyFlows 运行目录 `public/templates/<模板 ID>/` 下的外部前端模板。模板是静态前端产物，由 AnyFlows 服务在运行时扫描、校验和切换；编译进 Rust 二进制的前端始终作为默认回退。

## 1. 开发与构建

模板必须能够脱离后端独立构建。进入前端工程后执行：

```bash
pnpm install --frozen-lockfile
pnpm build
```

`pnpm build` 只需要 Node.js、pnpm 和前端依赖，不需要启动 AnyFlows，也不需要访问数据库。构建输出目录默认为 `dist/`；发布时复制 `dist/` 中的文件，不要复制 `node_modules`、源码、环境文件或密钥。

本地联调可以启动 Vite 开发服务器。`VITE_API_PROXY_TARGET` 指定 `/api` 和 `/v1` 的代理目标，默认是 `http://127.0.0.1:8085`。需要把构建产物部署到独立域名时，在构建环境设置 `VITE_API_BASE_URL`，例如 `https://api.example.com`；同源部署保持为空，浏览器会使用当前域名。API 地址不能写入模板源码中的固定生产密钥或私有地址。

## 2. 模板包格式

每个模板是模板根目录下的一个子目录：

```text
public/templates/
└── dark-console/
    ├── template.json
    ├── index.html
    └── assets/
        ├── app-<hash>.js
        └── app-<hash>.css
```

`template.json` 是必需的元数据文件，格式如下：

```json
{
  "schema_version": 1,
  "id": "dark-console",
  "name": "Dark Console",
  "version": "1.0.0",
  "api_contract": "0.2",
  "entry": "index.html",
  "description": "为团队提供简洁的数据控制台",
  "author": "Template Team",
  "preview": "preview.png"
}
```

约束如下：

- `schema_version` 必须为 `1`，`id` 必须与目录名一致。
- 目录名和 ID 只能包含 ASCII 小写字母、数字、`-`、`_`，长度不超过 64 个字符。
- `embedded`（Classic）和 `embedded-next`（Next）是内置模板保留 ID，外部模板不得使用。
- `description`、`author`、`preview` 为可选字段；旧的元数据文件仍然兼容。描述最多 1,000 字节，作者最多 160 字节，不允许控制字符。
- `preview` 为模板目录内的相对图片路径，支持 PNG、JPEG、WebP，最大 2 MiB；拒绝路径穿越、远程地址、HTML/SVG 和不匹配的文件签名。图片不存在或无效时，模板标记无效并禁止启用。
- `name`、`version`、`api_contract` 必须是非空且不包含控制字符的短文本；`entry` 必须为 `index.html`。
- `index.html` 必须存在，所有文件必须位于当前模板目录内。服务会拒绝目录穿越、符号链接、特殊文件以及超出文件数量、单文件大小或总大小限制的模板。
- 模板应包含完整的构建产物；启用外部模板后，缺失资源会按 404 处理，不会从内嵌前端拼接资源。
- 模板只提供静态文件，不会在服务端执行 JavaScript、脚本或其他代码。

## 3. API 契约与 OpenAPI

OpenAPI 契约入口：

- 仓库文件：[web/openapi/openapi.json](../web/openapi/openapi.json)
- Gitea `develop` 分支：[在线查看](https://gitea.acmecloud.cn/acmecloud/AnyFlows/src/branch/develop/web/openapi/openapi.json) · [下载原始 JSON](https://gitea.acmecloud.cn/acmecloud/AnyFlows/raw/branch/develop/web/openapi/openapi.json)

当前服务不提供 `/openapi.json`、Swagger UI 或 ReDoc HTTP 路由；部署后的站点地址不是 OpenAPI 文档地址。模板开发应下载与后端版本对应的仓库文件，并以其中的路径、请求体、响应体和鉴权要求为准。

OpenAPI 的源定义位于 Rust 后端的 `crates/af-http/src/openapi/` 及相关路由类型。需要更新接口时，先修改 Rust 契约，再按以下顺序导出和校验：

~~~bash
# 导出 web/openapi/openapi.json
cargo run --locked --package xtask -- openapi export

# 生成 web/src/lib/api/generated/ 下的 TypeScript 客户端
corepack pnpm@10.33.0 --dir web api:generate

# 提交前检查契约和生成客户端是否漂移
cargo run --locked --package xtask -- openapi check
corepack pnpm@10.33.0 --dir web api:check
~~~

不要直接编辑 `web/openapi/openapi.json` 或 `web/src/lib/api/generated/` 下的生成文件。模板应声明所需的 `api_contract`，并在发布前确认它兼容当前服务版本；接口新增字段通常可以向后兼容，但删除、改名、改变枚举或必填性都应提升契约版本并重新验证模板。

生产模板优先使用同源相对请求：`/api/...`、`/v1/...`。跨域构建使用 `VITE_API_BASE_URL`，并同时配置服务端 CORS 白名单。模板不得把管理员令牌、上游 API key 或数据库凭据打进静态资源。

## 4. 发布、扫描与切换

发布时复制 `dist/` **里面的内容** 到 `public/templates/<id>/`，不要多套一层 `dist/`：

~~~text
public/templates/dark-console/template.json  # 正确
public/templates/dark-console/index.html
public/templates/dark-console/assets/...

public/templates/dark-console/dist/index.html # 错误：扫描器不会把 dist 当作入口
~~~

`server.frontend_template_directory` 和 `AF_SERVER__FRONTEND_TEMPLATE_DIRECTORY` 必须是运行目录下的相对路径。默认值是 `public/templates`。systemd 的 `WorkingDirectory`、Docker 的工作目录、运行用户权限以及模板目录挂载点必须与这个相对路径一致；目录不存在、挂载到了宿主机的其他位置或服务用户不可读时，扫描结果会为空。

文件复制完成后，在管理员界面执行重新扫描。扫描通过后模板才会出现在列表中；服务会在扫描阶段把整套资源读入内存，切换时原子替换资源快照。直接修改磁盘文件不会自动更新正在运行的快照，修改后必须重新扫描，必要时重启服务确认启动扫描结果。

当前选择会持久化到数据库，服务重启后继续使用。删除、改名或损坏已选模板后重新扫描，服务会回退到内嵌前端并记录警告；修复后需再次扫描并重新选择。回滚时在管理员设置中切回 `embedded`（请求体中的 `template_id` 为空或 `embedded`）。切换只影响新的静态资源请求，不会中断正在进行的 API 请求。

外部模板通过站点根路径提供，模板中的资源应使用 `/assets/...` 等根相对路径，不要把资源地址写成 `/templates/<id>/...`。`/api`、`/v1`、`/healthz`、`/readyz` 和 `/metrics` 始终由后端路由处理，不会回退到模板 HTML；带扩展名的未知资源也会直接返回 404。

## 5. 运行时配置与缓存

同源生产环境不需要额外配置。需要把同一份模板连接到不同后端时，使用构建变量：

```bash
VITE_API_BASE_URL=https://api.example.com pnpm build
```

服务端模板目录可通过 `server.frontend_template_directory` 配置，或使用环境变量
`AF_SERVER__FRONTEND_TEMPLATE_DIRECTORY` 覆盖；路径必须是运行目录下的相对路径。

模板服务对 `assets/` 下的资源使用一年 `immutable` 缓存，对 `index.html` 和其他非哈希入口使用重新验证缓存。每次发布都要让带哈希的 JS/CSS 文件名变化，并同步更新 `index.html` 的引用；只覆盖同名资源可能导致浏览器继续使用旧版本。

同源模板优先使用 `/api/...`、`/v1/...` 等相对地址。跨域模板必须在构建时设置 `VITE_API_BASE_URL`，并在服务端配置对应的 CORS 白名单；同时验证带凭据请求、预检请求和流式响应。模板不得假设后端一定运行在开发端口，也不得读取本地文件系统或把管理员令牌、上游 API key、数据库凭据打进静态资源。

后续版本可以提供运行时 `window.__ANYFLOWS_CONFIG__ = { apiBaseUrl: "" }` 配置；空字符串表示当前域名。若模板自行实现运行时配置，必须保证生产环境不会把服务端密钥注入浏览器。

## 6. 限制与安全边界

扫描器会严格校验 `template.json`（拒绝未知字段），并要求 `schema_version` 为 `1`、`id` 与目录名完全一致、`entry` 为 `index.html`。模板根目录及其内容不得包含符号链接、特殊文件或目录穿越路径。当前限制为：单个资源最多 8 MiB、整套模板最多 64 MiB、最多 4096 个资源，`template.json` 最多 64 KiB。

外部模板只提供静态文件，服务端不会执行其中的脚本。启用外部模板后，资源必须完整包含在该模板内，缺失资源会返回 404，不会从内嵌前端拼接；因此不要只发布一个修改过的 `index.html`。

## 7. 测试清单

发布前至少验证：

1. 在没有启动后端的环境执行 `pnpm build`。
2. 使用同源 AnyFlows 服务打开登录、登出、模型广场、管理页面和 Playground。
3. 使用非空 `VITE_API_BASE_URL` 构建并验证登录、鉴权失败、CORS 和流式响应。
4. 直接刷新 `/console/...` 等深链接，确认服务回退到 `index.html`。
5. 检查带哈希资源的缓存头、HTML 的重新验证行为和不存在资源的 404。
6. 在管理员界面执行扫描、切换、回退，并重启服务确认选择持久化。
7. 使用一个缺少 `index.html`、包含符号链接、未知元数据字段或非法 JSON 的目录，确认扫描报告错误且内嵌前端仍可访问。
8. 在 systemd/Docker 的实际运行用户和工作目录下重复扫描，确认挂载路径、文件权限和相对路径配置一致。

模板发布应与后端 API 契约版本一起记录。升级 `api_contract` 前先完成向后兼容验证，再在模板元数据中更新版本。

## 7. 双内置前端与模板目录

服务二进制同时内嵌 `web/`（AnyFlows Classic）和 `web-next/`（AnyFlows Next）。两套前端共用后端 API、权限与站点设置，分别保留经典紧凑风格和 HeroUI 新版风格。发布前必须构建两套前端；Docker、Gitea 发布包和 CNB tag 发布均包含两套产物。

管理员在站点设置的模板目录中查看内置/外部分类、搜索名称/ID/作者、选择每页 8/12/24 个模板。默认只显示 8 张卡片，缩略图接近视口时才请求；目录缓存 60 秒，分页复用目录，图片短期缓存并在重新扫描后刷新。普通用户和组织用户不能访问模板管理或预览接口。

预览是静态图片，不会执行模板代码、读取业务数据或改变全站前端。内置模板附带明确标注的布局示意图；外部模板显示作者提供的图片。点击启用后必须确认，服务持久化选择并立即切换，当前页面随后刷新。`null` 或 `embedded` 保持原来的 Classic 回退语义，`embedded-next` 选择新版；重启后恢复选择，重新扫描不会丢失内置选择。外部模板损坏或消失时回退 Classic。

模板包本身仍应来自可信来源：静态预览的隔离并不意味着启用后的第三方 JavaScript 经过安全审计。

## 8. 双前端契约与远程质量验证

`cargo xtask openapi export` 同时写入两套前端的 OpenAPI 文档。生成客户端与共享的分页、厂商、渠道和日志逻辑由 `tools/ci/check-frontend-parity.mjs` 校验，界面组件允许按风格分别实现。

Gitea CI 对 `web` 和 `web-next` 分别执行锁定依赖安装、脚本测试、API 生成一致性检查、lint、TypeScript 和生产构建。`Frontend generated assets` 工作流在远程生成契约、依赖锁文件及 Rust 格式调整，以 gzip/base64 分行补丁输出供开发者审查；该工作流只有读取权限，不会推送或合并代码。源码补丁不含编译产物，应用后仍需完整 CI 验证。
