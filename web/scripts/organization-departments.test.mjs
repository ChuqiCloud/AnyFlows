import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { URL } from 'node:url'

import { routeFromHash } from '../src/app-route.ts'

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8')

test('部门目录路由可解析并保持独立入口', () => {
  assert.deepEqual(routeFromHash('#/console/organization-departments'), {
    view: 'organization-departments',
  })
})

test('部门目录前端覆盖读取、创建、移动和不可逆禁用', () => {
  const page = read('../src/features/organization-departments/organization-department-page.tsx')
  const api = read('../src/features/organization-departments/organization-department-api.ts')

  assert.match(page, /organization\.departments\.read/)
  assert.match(page, /organization\.departments\.manage/)
  assert.match(page, /expected_version/)
  assert.match(page, /不可逆|disable|disabled/i)
  assert.match(api, /listOrganizationDepartments/)
  assert.match(api, /createOrganizationDepartment/)
  assert.match(api, /moveOrganizationDepartment/)
  assert.match(api, /disableOrganizationDepartment/)
})

test('部门目录 API 已登记四个路径、权限和稳定游标', () => {
  const openapi = JSON.parse(read('../openapi/openapi.json'))
  const openapiSource = read('../../crates/af-http/src/openapi/organizations.rs')
  const http = read('../../crates/af-http/src/organizations.rs')
  const domain = read('../../crates/af-domain/src/organization.rs')

  for (const path of [
    '/api/organizations/{organization_id}/departments',
    '/api/organizations/{organization_id}/departments/{department_id}',
    '/api/organizations/{organization_id}/departments/{department_id}/move',
    '/api/organizations/{organization_id}/departments/{department_id}/disable',
  ]) {
    assert.ok(openapi.paths[path], '缺少 OpenAPI 路径 ' + path)
  }
  assert.ok(openapi.paths['/api/organizations/{organization_id}/departments'].get)
  assert.ok(openapi.paths['/api/organizations/{organization_id}/departments'].post)
  assert.ok(openapi.paths['/api/organizations/{organization_id}/departments/{department_id}'].get)
  assert.ok(openapi.paths['/api/organizations/{organization_id}/departments/{department_id}/move'].post)
  assert.ok(openapi.paths['/api/organizations/{organization_id}/departments/{department_id}/disable'].post)
  assert.match(http, /DepartmentsRead/)
  assert.match(http, /DepartmentsManage/)
  assert.match(openapiSource, /sort_order:id/)
  assert.match(domain, /organization\.departments\.read/)
  assert.match(domain, /organization\.departments\.manage/)
  assert.ok(openapi.components.schemas.OrganizationDepartmentResponse)
})

test('部门目录提供中英文界面文案', () => {
  const zh = JSON.parse(read('../src/i18n/locales/zh.json'))
  const en = JSON.parse(read('../src/i18n/locales/en.json'))

  for (const locale of [zh, en]) {
    assert.equal(typeof locale.organizationDepartments.title, 'string')
    assert.equal(typeof locale.organizationDepartments.actions.create, 'string')
    assert.equal(typeof locale.organizationDepartments.actions.move, 'string')
    assert.equal(typeof locale.organizationDepartments.actions.disable, 'string')
  }
})
