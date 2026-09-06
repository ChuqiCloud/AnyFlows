import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))

test('公告 OpenAPI 覆盖公开读取与管理员版本操作', () => {
  const expected = {
    '/api/announcements': ['listPublicAnnouncements'],
    '/api/admin/announcements': ['listAdminAnnouncements', 'createAdminAnnouncement'],
    '/api/admin/announcements/{id}': ['updateAdminAnnouncement'],
    '/api/admin/announcements/{id}/publish': ['publishAdminAnnouncement'],
    '/api/admin/announcements/{id}/revoke': ['revokeAdminAnnouncement'],
  }
  for (const [path, operationIds] of Object.entries(expected)) {
    assert.ok(openapi.paths[path], `缺少公告路径 ${path}`)
    const actual = Object.values(openapi.paths[path]).map((operation) => operation.operationId)
    assert.deepEqual(actual, operationIds)
  }
})

test('公告 schema 保留状态、版本、受众和可见时间窗', () => {
  const announcement = openapi.components.schemas.Announcement
  assert.ok(announcement)
  for (const field of ['id', 'version', 'status', 'audience', 'title_zh', 'title_en', 'body_zh', 'body_en', 'visible_from', 'visible_until']) {
    assert.ok(announcement.properties[field], `公告缺少字段 ${field}`)
  }
  assert.match(JSON.stringify(announcement.properties.status), /draft/)
  assert.match(JSON.stringify(announcement.properties.status), /published/)
  assert.match(JSON.stringify(announcement.properties.status), /revoked/)
  assert.deepEqual(openapi.components.schemas.AnnouncementAudience.enum, ['public', 'authenticated'])
})

test('公告前端接入公开首页、管理员路由与操作页面', async () => {
  const [landing, routes, app, navigation, page, strip] = await Promise.all([
    readFile(new URL('../src/features/landing/landing-page.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/app-route.ts', import.meta.url), 'utf8'),
    readFile(new URL('../src/App.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/components/layout/app-navigation.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/announcements/announcement-page.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/announcements/announcement-strip.tsx', import.meta.url), 'utf8'),
  ])
  assert.match(landing, /AnnouncementStrip/)
  assert.match(routes, /system-settings\/announcements/)
  assert.match(app, /AnnouncementPage/)
  assert.doesNotMatch(app, /requireAdmin[\s\S]*?view !== ['"]announcements['"]/)
  assert.match(navigation, /announcements.*system-settings\/announcements/s)
  assert.match(page, /expected_version/)
  assert.match(page, /publish|revoke/)
  assert.match(page, /audience/)
  assert.match(strip, /usePublicAnnouncements/)
})

test('公告中英文文案完整且没有空占位符', async () => {
  for (const locale of ['zh', 'en']) {
    const messages = JSON.parse(await readFile(new URL(`../src/i18n/locales/${locale}.json`, import.meta.url), 'utf8')).announcements
    assert.ok(messages)
    for (const value of [
      messages.eyebrow,
      messages.title,
      messages.subtitle,
      messages.public.label,
      messages.status.draft,
      messages.status.published,
      messages.status.revoked,
      messages.list.title,
      messages.list.description,
      messages.list.empty,
      messages.editor.new,
      messages.editor.edit,
      messages.editor.description,
      messages.fields.titleZh,
      messages.fields.audience,
      messages.fields.titleEn,
      messages.fields.bodyZh,
      messages.fields.bodyEn,
      messages.fields.visibleFrom,
      messages.fields.visibleUntil,
      messages.audience.public,
      messages.audience.authenticated,
      messages.actions.refresh,
      messages.actions.retry,
      messages.actions.new,
      messages.actions.create,
      messages.actions.save,
      messages.actions.publish,
      messages.actions.revoke,
      messages.errors.load,
      messages.errors.save,
      messages.errors.transition,
    ]) {
      assert.equal(typeof value, 'string')
      assert.notEqual(value.trim(), '')
      assert.doesNotMatch(value, /\ufffd|\?{2,}/)
    }
  }
})
