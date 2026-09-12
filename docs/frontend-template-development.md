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
  "entry": "index.html"
}
```

约束如下：

- `schema_version` 必须为 `1`，`id` 必须与目录名一致。
- 目录名和 ID 只能包含 ASCII 小写字母、数字、`-`、`_`，长度不超过 64 个字符。
- `name`、`version`、`api_contract` 必须是非空且不包含控制字符的短文本；`entry` 必须为 `index.html`。
- `index.html` 必须存在，所有文件必须位于当前模板目录内。服务会拒绝目录穿越、符号链接、特殊文件以及超出文件数量、单文件大小或总大小限制的模板。
- 模板应包含完整的构建产物；启用外部模板后，缺失资源会按 404 处理，不会从内嵌前端拼接资源。
- 模板只提供静态文件，不会在服务端执行 JavaScript、脚本或其他代码。

## 3. API 契约

前端 API 以仓库中的 `web/openapi/openapi.json` 为准。需要更新接口时，先修改后端 OpenAPI 定义，再运行生成命令更新客户端；不要直接编辑 `web/src/lib/api/generated/` 下的生成文件。模板应声明所需的 `api_contract`，并在发布前确认其兼容当前服务版本。

生产模板优先使用同源相对请求：`/api/...`、`/v1/...`。跨域构建使用 `VITE_API_BASE_URL`，并同时配置服务端 CORS 白名单。模板不得把管理员令牌、上游 API key 或数据库凭据打进静态资源。

## 4. 发布、扫描与切换

将构建好的 `dist/` 内容复制到 `public/templates/<id>/` 后，在管理员界面执行重新扫描。扫描通过后才会出现在模板列表中；选择模板时服务会先完整加载并校验资源，再原子替换当前静态资源快照。当前选择会持久化，服务重启后继续使用；选择不存在或损坏时自动回退到内嵌前端并记录警告。

外部模板通过站点根路径提供，模板中的资源应使用 `/assets/...` 等根相对路径，不要把资源地址写成 `/templates/<id>/...`。`/api`、`/v1`、`/healthz`、`/readyz` 和 `/metrics` 始终由后端路由处理，不会被模板的 `index.html` 覆盖。

回滚时在管理员设置中切回 `embedded`，或修复/删除模板后重新扫描。切换只影响新的静态资源请求，不会中断正在进行的 API 请求。

## 5. 运行时配置

同源生产环境不需要额外配置。需要把同一份模板连接到不同后端时，使用构建变量：

```bash
VITE_API_BASE_URL=https://api.example.com pnpm build
```

服务端模板目录可通过 `server.frontend_template_directory` 配置，或使用环境变量
`AF_SERVER__FRONTEND_TEMPLATE_DIRECTORY` 覆盖；路径必须是运行目录下的相对路径。

后续版本可以提供运行时 `window.__ANYFLOWS_CONFIG__ = { apiBaseUrl: "" }` 配置；空字符串表示当前域名。模板不得假设后端一定运行在开发端口，也不得读取本地文件系统。

## 6. 测试清单

发布前至少验证：

1. 在没有启动后端的环境执行 `pnpm build`。
2. 使用同源 AnyFlows 服务打开登录、登出、模型广场、管理页面和 Playground。
3. 使用非空 `VITE_API_BASE_URL` 构建并验证登录、鉴权失败、CORS 和流式响应。
4. 直接刷新 `/console/...` 等深链接，确认服务回退到 `index.html`。
5. 检查带哈希资源的缓存头、HTML 的重新验证行为和不存在资源的 404。
6. 在管理员界面执行扫描、切换、回退，并重启服务确认选择持久化。
7. 使用一个缺少 `index.html`、包含符号链接或非法 JSON 的目录，确认扫描报告错误且内嵌前端仍可访问。

模板发布应与后端 API 契约版本一起记录。升级 `api_contract` 前先完成向后兼容验证，再在模板元数据中更新版本。
