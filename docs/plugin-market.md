# 插件市场

插件市场只负责发现。安装仍由 AnyFlows 的插件安装器完成，安装器会校验压缩包、清单、SHA-256、兼容版本和部署授权。

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

## 推荐发布方式

生产环境发布 GitHub Release 的 `.afplugin` 文件，并在同一 Release 上传 `.sha256` 文件。GitHub Action 可以生成压缩包、SHA-256 和索引；服务端只下载已发布的包，不在服务器执行仓库脚本。

从 Git 仓库安装时必须固定完整 commit，并指定插件子目录。分支名和短 commit 不能作为生产安装来源。受限仓库通过服务端 GitHub App 或部署凭据访问，浏览器不提交 GitHub PAT。

## 安装流程

1. 管理员配置市场源并刷新索引。
2. 在市场页按插件、作者、能力和兼容版本筛选。
3. 选择版本后展示仓库、commit、能力和 SHA-256。
4. 服务端下载并校验包，复用现有插件审批和部署流程。
5. 多节点只由一个节点下载，其他节点按 artifact digest 从共享分发库恢复。

市场索引不会绕过插件审批、能力授权、部署代次或节点 ready 检查。
