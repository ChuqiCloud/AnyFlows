/** 服务端允许写入企业自定义角色的闭合权限集合。 */
export const CUSTOM_ROLE_PERMISSION_CODES = [
  'organization.overview.read',
  'organization.profile.write',
  'organization.members.manage',
  'organization.teams.manage',
  'organization.wallet.read',
  'organization.wallet.manage',
  'organization.budgets.manage',
  'organization.keys.read_all',
  'organization.keys.manage_self',
  'organization.usage_logs.read_all',
  'organization.usage_logs.read_self',
  'organization.audit.read',
  'organization.roles.read',
] as const

export type CustomRolePermissionCode = (typeof CUSTOM_ROLE_PERMISSION_CODES)[number]

/** 按服务端白名单排序并丢弃异常权限，避免客户端回传越权权限码。 */
export function selectedCustomRolePermissions(permissions: readonly string[]): CustomRolePermissionCode[] {
  return CUSTOM_ROLE_PERMISSION_CODES.filter((permission) => permissions.includes(permission))
}
