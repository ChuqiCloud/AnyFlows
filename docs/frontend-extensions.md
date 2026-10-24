# 前端扩展

Classic 和 Next 从各自的 `src/extensions/modules/*/index.ts` 加载构建时扩展。社区仓库提供扩展接口，发行版在构建前装配模块源码。

## 页面与导航

模块导出 `ConsoleExtension`，包含唯一 ID、翻译资源和懒加载路由。路由须位于 `/console/extensions/<name>`，声明 `user` 或 `admin` 角色及至少一个必需能力。路径必须唯一。

Classic 使用 hash 路由，Next 使用浏览器路由。扩展页挂在宿主已有的初始化、会话和控制台边界内，共享 API 客户端、查询缓存及登录处理。

`GET /api/extensions` 返回页面所需的全部能力后显示导航。直接访问页面时，分别处理能力加载、请求失败和能力缺失状态。后端接口独立校验成员和资源权限。

路由的 `navigation` 可声明分组 ID、翻译标题、图标和排序值。宿主按角色及能力过滤后分组；未声明时使用默认分组。

用户路由可声明 `entryPoint: 'account-verification'`，作为实名认证完成后的扩展入口。同一入口只能声明一次，管理员路由不能使用。模块不可用时隐藏入口。

## 扩展登录

`loginMethods` 声明方法 ID、翻译标签、图标和懒加载表单。方法 ID 在模块内唯一。宿主提供返回操作及注册开放状态。支持回调的方法实现 `exchangeTicket`，兑换后返回 `LoginResponse`，由宿主建立会话。

浏览器回调格式为：

```text
/#/oauth/callback?method=<module>:<method>&ticket=<ticket>
```

Next 同时支持该 fragment 和浏览器回调路由。宿主读取并清除回调参数后兑换 ticket；除 `ticket`、`error`、`method` 外的单值参数作为上下文交给对应方法。未知或重复方法会被拒绝。

模块复用宿主的会话存储。ticket、受保护响应和凭据不得写入查询缓存或浏览器持久化存储。

## 构建与接口

发行仓库装配模块源码、合并 OpenAPI、生成客户端并构建两套前端。私有模块及其生成契约保留在发行仓库。扩展页需适配各自主题，覆盖加载、空数据、失败和移动端状态。

外部静态模板的目录与部署格式见 [前端模板](frontend-template-development.md)。
