import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  buildNetworkSettingsSchema,
  networkSettingsValues,
  toNetworkSettingsRequest,
} from '../src/features/network-settings/network-settings-form-model.ts'

const messages = {
  host: 'host',
  port: 'port',
  username: 'username',
  password: 'password',
  passwordRequired: 'password required',
  passwordWithoutUser: 'password without user',
}

const persisted = {
  mode: 'inherit',
  proxy_host: null,
  proxy_port: null,
  username: null,
  password_configured: false,
  trust_proxy_dns: false,
  version: 1,
}

test('网络设置导航在中英文资源中都有可读文案', () => {
  for (const locale of ['zh', 'en']) {
    const messages = JSON.parse(readFileSync(new URL(`../src/i18n/locales/${locale}.json`, import.meta.url), 'utf8'))
    assert.equal(typeof messages.nav.networkSettings, 'string')
    assert.notEqual(messages.nav.networkSettings, 'nav.networkSettings')
  }
})

test('网络设置路由指向管理员系统设置子域', () => {
  assert.deepEqual(routeFromHash('#/console/system-settings/network'), { view: 'network-settings' })
})

test('继承和直连模式不提交代理字段', () => {
  const values = networkSettingsValues(persisted)
  assert.equal(buildNetworkSettingsSchema(false, messages).safeParse(values).success, true)
  assert.deepEqual(toNetworkSettingsRequest({
    ...values,
    mode: 'direct',
    trustProxyDns: false,
  }), {
    mode: 'direct',
    proxy_host: null,
    proxy_port: null,
    username: null,
    password: null,
    trust_proxy_dns: false,
  })
})

test('代理模式要求主机、端口和首次认证密码', () => {
  const values = networkSettingsValues(persisted)
  const schema = buildNetworkSettingsSchema(false, messages)
  assert.equal(schema.safeParse({ ...values, mode: 'socks5h' }).success, false)
  assert.equal(schema.safeParse({
    ...values,
    mode: 'socks5h',
    proxyHost: 'proxy.example.com',
    proxyPort: 1080,
    username: 'proxy-user',
    password: 'proxy-secret',
    trustProxyDns: true,
  }).success, true)
})
