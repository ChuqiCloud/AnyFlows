/**
 * 控制台访问级别判定统一由平台权限层提供，
 * 业务路由只做转发，避免同一套规则散落在业务代码里。
 */
export { requiresAdmin } from '@/shared/access-control'
