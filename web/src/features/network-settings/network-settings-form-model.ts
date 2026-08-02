import { z } from 'zod'

import type {
  AdminNetworkSettings,
  AdminNetworkSettingsMode,
  AdminNetworkSettingsRequestWritable,
} from '@/lib/api/generated/types.gen'

export type NetworkSettingsValues = {
  mode: AdminNetworkSettingsMode
  proxyHost: string
  proxyPort: number
  username: string
  password: string
  trustProxyDns: boolean
}

type ValidationMessages = {
  host: string
  port: string
  username: string
  password: string
  passwordRequired: string
  passwordWithoutUser: string
}

const proxyModes: readonly AdminNetworkSettingsMode[] = ['http', 'https', 'socks5', 'socks5h']

/** 从脱敏网络设置投影构造表单值，密码始终从空字符串开始。 */
export function networkSettingsValues(settings: AdminNetworkSettings): NetworkSettingsValues {
  return {
    mode: settings.mode,
    proxyHost: settings.proxy_host ?? '',
    proxyPort: settings.proxy_port ?? 1080,
    username: settings.username ?? '',
    password: '',
    trustProxyDns: settings.trust_proxy_dns,
  }
}

/** 校验代理结构，并让继承/直连模式自动忽略代理字段。 */
export function buildNetworkSettingsSchema(
  passwordConfigured: boolean,
  messages: ValidationMessages,
) {
  return z.object({
    mode: z.enum(['inherit', 'direct', 'http', 'https', 'socks5', 'socks5h']),
    proxyHost: z.string(),
    proxyPort: z.number().int(messages.port).min(1, messages.port).max(65_535, messages.port),
    username: z.string(),
    password: z.string(),
    trustProxyDns: z.boolean(),
  }).superRefine((values, context) => {
    const usesProxy = proxyModes.includes(values.mode)
    if (usesProxy && (!values.proxyHost || !isValidProxyHost(values.proxyHost))) {
      addIssue(context, 'proxyHost', messages.host)
    }
    if (!usesProxy && values.trustProxyDns) {
      addIssue(context, 'trustProxyDns', messages.host)
    }
    if (!isValidOptionalText(values.username, 320)) {
      addIssue(context, 'username', messages.username)
    }
    if (values.password && !isValidPassword(values.password)) {
      addIssue(context, 'password', messages.password)
    }
    if (usesProxy && values.username && !values.password && !passwordConfigured) {
      addIssue(context, 'password', messages.passwordRequired)
    }
    if (!values.username && values.password) {
      addIssue(context, 'password', messages.passwordWithoutUser)
    }
  })
}

/** 将表单值转换为结构化请求，空密码表示保留已有密文。 */
export function toNetworkSettingsRequest(
  values: NetworkSettingsValues,
): AdminNetworkSettingsRequestWritable {
  const usesProxy = proxyModes.includes(values.mode)
  return {
    mode: values.mode,
    proxy_host: usesProxy ? values.proxyHost : null,
    proxy_port: usesProxy ? values.proxyPort : null,
    username: usesProxy ? values.username || null : null,
    password: usesProxy && values.password ? values.password : null,
    trust_proxy_dns: usesProxy && values.trustProxyDns,
  }
}

export function isProxyMode(mode: AdminNetworkSettingsMode) {
  return proxyModes.includes(mode)
}

function isValidProxyHost(value: string) {
  const bytes = new TextEncoder().encode(value).length
  return bytes > 0
    && bytes <= 255
    && value.trim() === value
    && /^[\x21-\x7e]+$/u.test(value)
    && !value.includes('://')
    && !/[\\/@]/u.test(value)
}

function isValidOptionalText(value: string, maximumBytes: number) {
  if (!value) return true
  return new TextEncoder().encode(value).length <= maximumBytes
    && value.trim() === value
    && !hasControlCharacter(value)
}

function isValidPassword(value: string) {
  const bytes = new TextEncoder().encode(value).length
  return bytes > 0 && bytes <= 4_096 && !hasControlCharacter(value)
}

function hasControlCharacter(value: string) {
  return [...value].some((character) => /\p{Cc}/u.test(character))
}

function addIssue(
  context: z.RefinementCtx,
  path: keyof NetworkSettingsValues,
  message: string,
) {
  context.addIssue({ code: 'custom', path: [path], message })
}
