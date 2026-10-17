# 前端扩展

Classic 和 Next 从 `src/extensions/modules/*/index.ts` 发现构建时模块。社区仓库不包含私有模块，发行版在构建前装配对应源码。

## 页面与导航

模块导出 `ConsoleExtension`，包含唯一 ID、翻译资源和懒加载路由。路由位于 `/console/extensions/<name>`，声明 `user` 或 `admin` 角色以及全部必需能力。重复路径或命名空间之外的路径会被拒绝。

Classic 使用 hash 路由，Next 使用浏览器路由。扩展页挂在宿主已有的初始化、会话和控制台边界内，共享 API 客户端、查询缓存及登录处理。

`GET /api/extensions` 返回所有所需能力后，才显示对应导航。直达已注册页面时，能力加载中、请求失败或能力不存在分别显示相应状态。能力发现只决定功能入口，后端仍需检查成员和资源权限。

路由的 `navigation` 可声明稳定分组 ID、翻译标题、图标和数值排序。宿主先按角色及能力过滤，再分组。未声明时使用默认分组。

用户路由可声明 `entryPoint: 'account-verification'`，作为认证完成后的可选入口；同一入口必须唯一，管理员路由不能占用。模块不可用时宿主不显示入口。

## 扩展登录

`loginMethods` 声明唯一方法 ID、翻译标签、图标和懒加载表单。宿主提供返回操作及当前注册是否开放的状态。支持回调的方法可实现 `exchangeTicket`，兑换后返回宿主的 `LoginResponse`，继续建立原有管理会话。

浏览器回调格式为：

```text
/#/oauth/callback?method=<module>:<method>&ticket=<ticket>
```

Next 同时支持上述 fragment 和自身的浏览器回调路由。宿主消费并清除参数后才交换 ticket；除 `ticket`、`error`、`method` 外的单值参数作为不透明上下文交给匹配方法。未知或重复方法被拒绝，不回退到内置 OAuth。

模块不能另建登录存储，也不能把 ticket、受保护响应或凭据放进查询缓存和浏览器持久化存储。

## 构建与接口

发行仓库负责私有源码装配、合并 OpenAPI、生成客户端和构建两套前端。私有模块与生成契约不提交到社区仓库。页面需适配各自主题，并处理加载、空数据、失败及移动端状态。

外部静态模板与此类源码模块不同，部署格式见 [前端模板](frontend-template-development.md)。