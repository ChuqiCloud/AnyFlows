import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8')
const openapi = JSON.parse(await read('../openapi/openapi.json'))

test('成员部门归属 OpenAPI 路径使用稳定 operation id', () => {
  const operations = [
    ['/api/organizations/{organization_id}/members/{membership_id}/department-memberships', 'get', 'listOrganizationMemberDepartmentMemberships'],
    ['/api/organizations/{organization_id}/members/{membership_id}/department-memberships', 'post', 'addOrganizationMemberDepartmentMembership'],
    ['/api/organizations/{organization_id}/members/{membership_id}/department-memberships/{department_id}/primary', 'post', 'setPrimaryOrganizationMemberDepartmentMembership'],
    ['/api/organizations/{organization_id}/members/{membership_id}/department-memberships/{department_id}', 'delete', 'removeOrganizationMemberDepartmentMembership'],
  ]
  for (const [path, method, operationId] of operations) {
    assert.equal(openapi.paths[path]?.[method]?.operationId, operationId)
  }
})

test('成员部门归属前端闭合权限、CAS 和查询失效', async () => {
  const [api, page, editor, http, admin] = await Promise.all([
    read('../src/features/organization-members/organization-member-api.ts'),
    read('../src/features/organization-members/organization-member-page.tsx'),
    read('../src/features/organization-members/organization-member-editor-sheet.tsx'),
    read('../../crates/af-http/src/organizations.rs'),
    read('../../crates/af-admin/src/organization_management.rs'),
  ])
  assert.match(api, /useOrganizationMemberDepartmentMemberships/)
  assert.match(api, /invalidateMemberDepartments/)
  assert.match(api, /addOrganizationMemberDepartmentMembership/)
  assert.match(api, /setPrimaryOrganizationMemberDepartmentMembership/)
  assert.match(api, /removeOrganizationMemberDepartmentMembership/)
  assert.match(page, /organization.departments.read/)
  assert.match(page, /organization.departments.manage/)
  assert.match(editor, /expected_membership_version: addAsPrimary \? member\.version : null/)
  assert.match(editor, /expected_membership_version: member.version/)
  assert.match(editor, /record.is_primary/)
  assert.match(http, /OrganizationPermission::DepartmentsRead/)
  assert.match(http, /OrganizationPermission::DepartmentsManage/)
  assert.match(admin, /set_primary_department_membership/)
  assert.match(admin, /OrganizationManagementError::Conflict/)
})

test('成员部门归属保留活动与历史状态的双语文案', async () => {
  const [zh, en] = await Promise.all([
    read('../src/i18n/locales/zh.json').then(JSON.parse),
    read('../src/i18n/locales/en.json').then(JSON.parse),
  ])
  for (const locale of [zh, en]) {
    assert.equal(typeof locale.organizationConsole.members.departments.title, 'string')
    assert.equal(typeof locale.organizationConsole.members.departments.add, 'string')
    assert.equal(typeof locale.organizationConsole.members.departments.primary, 'string')
    assert.equal(typeof locale.organizationConsole.members.departments.status.active, 'string')
    assert.equal(typeof locale.organizationConsole.members.departments.status.removed, 'string')
  }
})
