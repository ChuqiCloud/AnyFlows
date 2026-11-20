# 插件市场

管理员可以从市场索引查找插件、查看版本和安装。AnyFlows 安装器负责校验压缩包、清单、SHA-256、兼容版本和部署授权。

## 市场索引

市场源提供一个 `anyflows-plugin-index.json` 文件，格式为 `format: 1`：

```json
{
  "format": 1,
  "title": "AnyFlows 社区插件",
  "description": "公开插件版本索引",
  "plugins": [
    {
      "id": "com.example.provider",
      "title": "Example Provider",
      "description": "Provider 适配器",
      "icon": "https://example.com/icon.svg",
      "author": "Example",
      "repository": "https://github.com/example/anyflows-provider",
      "versions": [
        {
          "version": "1.2.0",
          "source": {
            "kind": "github_release",
            "archiveUrl": "https://github.com/example/anyflows-provider/releases/download/v1.2.0/provider.afplugin",
            "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "repository": "https://github.com/example/anyflows-provider",
            "commit": "0123456789abcdef0123456789abcdef01234567",
            "path": "dist/provider.afplugin"
          },
          "archiveUrl": "https://github.com/example/anyflows-provider/releases/download/v1.2.0/provider.afplugin",
          "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
          "commit": "0123456789abcdef0123456789abcdef01234567",
          "engines": { "anyflows": ">=0.3.0-beta.10" },
          "capabilities": ["provider.lifecycle"],
          "publishedAt": "2026-11-19T00:00:00Z"
        }
      ]
    }
  ]
}
```

`archiveUrl` 必须使用 HTTPS，`sha256` 是发布包的完整 SHA-256，`commit` 必须是 40 位完整 Git commit。索引中的 GitHub organization、仓库标签和作者信息都不代表 AnyFlows 官方认证；官方标识只由宿主配置的签名公钥决定。

`source` 是新格式。它的 `kind` 可以是 `github_release`、`git_artifact` 或 `http_archive`。`source.sha256` 必须与版本顶层的 `sha256` 完全一致。`github_release` 必须提供仓库地址，`git_artifact` 必须固定完整 commit。`path` 只用于记录制品在仓库中的位置，安装器不会执行仓库构建脚本。

## 推荐发布方式

生产环境发布 GitHub Release 的 `.afplugin` 文件，并在同一 Release 上传 `.sha256` 文件。GitHub Action 可以生成压缩包、SHA-256 和索引；服务端只下载已发布的包，不在服务器执行仓库脚本。

从 Git 仓库安装时必须固定完整 commit，并指定插件子目录。分支名和短 commit 不能作为生产安装来源。受限仓库通过服务端 GitHub App 或部署凭据访问，浏览器不提交 GitHub PAT。

## 安装流程

1. 在「插件管理 → 插件市场」输入索引的 HTTPS 地址并加载。
2. 按插件名称、ID 或作者查找，打开版本列表查看变更记录、来源、能力和 SHA-256。
3. 选择版本后安装。更新时留空配置会沿用当前部署，填写 JSON 则用新配置替换。
4. 宿主下载并校验制品。需要额外权限的能力由管理员在部署时明确授权。
5. 在部署列表查看节点的就绪状态。集群中的其他节点从共享制品库恢复同一制品。

市场索引不会绕过插件审批、能力授权、部署代次或节点 ready 检查。

登录用户可以先读取远程索引，服务端会校验 HTTPS 地址、索引大小、版本来源和 SHA-256 约束：

```http
GET /api/account/plugins/market/index?url=https%3A%2F%2Fexample.com%2Fanyflows-plugin-index.json
```

这个接口只做发现，不会安装或执行插件。市场页面可以把返回版本的 `source` 原样提交给管理员安装接口。

## 从 GitHub 安装

管理员可从 GitHub Release 资产或固定 commit 下的预构建 `.afplugin` 安装。首次安装传 `expected_generation: null`，更新时传当前部署代次。部署代次冲突时，刷新后重新确认版本和配置。更新时省略 `config` 可保留当前配置。

对应接口为：

```http
POST /api/admin/plugins/market/install
Authorization: Bearer <session-or-admin-pat>
Content-Type: application/json

{
  "plugin_id": "org.example.catalog",
  "plugin_version": "1.2.0",
  "source": {
    "kind": "github_release",
    "archive_url": "https://github.com/example/catalog/releases/download/v1.2.0/catalog.afplugin",
    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "repository": "https://github.com/example/catalog",
    "commit": "0123456789abcdef0123456789abcdef01234567",
    "path": null
  },
  "expected_generation": null
}
```

安装接口只接受管理员会话或具备对应管理员 PAT 权限的请求。下载完成后仍会检查包大小、ZIP 路径、重复文件、可执行文件、清单、Schema 和 SHA-256，然后进入审批、部署代次和多节点分发流程。签名不是自有插件安装的强制条件；已配置的官方签名只用于显示官方来源。
