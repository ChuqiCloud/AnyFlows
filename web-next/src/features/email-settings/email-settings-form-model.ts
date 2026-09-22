import { z } from 'zod'

import type {
  AdminEmailSettings,
  AdminEmailSettingsRequestWritable,
} from '@/lib/api/generated/types.gen'

export type EmailSettingsValues = {
  enabled: boolean
  host: string
  port: number
  tlsMode: 'start_tls' | 'tls'
  username: string
  password: string
  fromAddress: string
  fromName: string
  replyTo: string
  timeoutSeconds: number
}

type ValidationMessages = {
  host: string
  port: string
  username: string
  password: string
  passwordRequired: string
  passwordWithoutUser: string
  fromAddress: string
  fromAddressRequired: string
  fromName: string
  replyTo: string
  timeout: string
}

/** 从服务端脱敏投影构造表单值，密码始终从空字符串开始。 */
export function emailSettingsValues(settings: AdminEmailSettings): EmailSettingsValues {
  return {
    enabled: settings.enabled,
    host: settings.host,
    port: settings.port,
    tlsMode: settings.tls_mode,
    username: settings.username ?? '',
    password: '',
    fromAddress: settings.from_address,
    fromName: settings.from_name ?? '',
    replyTo: settings.reply_to ?? '',
    timeoutSeconds: settings.timeout_seconds,
  }
}

/** 校验结构化 SMTP 表单，并结合服务端状态判定旧密码是否可保留。 */
export function buildEmailSettingsSchema(
  passwordConfigured: boolean,
  messages: ValidationMessages,
) {
  return z.object({
    enabled: z.boolean(),
    host: z.string(),
    port: z.number().int(messages.port).min(1, messages.port).max(65_535, messages.port),
    tlsMode: z.enum(['start_tls', 'tls']),
    username: z.string(),
    password: z.string(),
    fromAddress: z.string(),
    fromName: z.string(),
    replyTo: z.string(),
    timeoutSeconds: z.number().int(messages.timeout).min(1, messages.timeout).max(60, messages.timeout),
  }).superRefine((values, context) => {
    if (values.host && !isValidHost(values.host)) {
      addIssue(context, 'host', messages.host)
    }
    if (values.enabled && !values.host) {
      addIssue(context, 'host', messages.host)
    }
    if (!isValidOptionalText(values.username, 320)) {
      addIssue(context, 'username', messages.username)
    }
    if (values.password && !isValidPassword(values.password)) {
      addIssue(context, 'password', messages.password)
    }
    if (values.username && !values.password && !passwordConfigured) {
      addIssue(context, 'password', messages.passwordRequired)
    }
    if (!values.username && values.password) {
      addIssue(context, 'password', messages.passwordWithoutUser)
    }
    if (values.fromAddress && !isValidEmail(values.fromAddress)) {
      addIssue(context, 'fromAddress', messages.fromAddress)
    }
    if (values.enabled && !values.fromAddress) {
      addIssue(context, 'fromAddress', messages.fromAddressRequired)
    }
    if (!isValidOptionalText(values.fromName, 128)) {
      addIssue(context, 'fromName', messages.fromName)
    }
    if (values.replyTo && !isValidEmail(values.replyTo)) {
      addIssue(context, 'replyTo', messages.replyTo)
    }
  })
}

/** 将表单值转换为完整覆盖请求；空密码表示保留，空用户名会关闭认证。 */
export function toEmailSettingsRequest(
  values: EmailSettingsValues,
): AdminEmailSettingsRequestWritable {
  return {
    enabled: values.enabled,
    host: values.host,
    port: values.port,
    tls_mode: values.tlsMode,
    username: values.username || null,
    password: values.password || null,
    from_address: values.fromAddress,
    from_name: values.fromName || null,
    reply_to: values.replyTo || null,
    timeout_seconds: values.timeoutSeconds,
  }
}

/** 复用后端邮件地址边界校验测试收件人，不在错误文案中回显原值。 */
export function isValidEmail(value: string) {
  const bytes = new TextEncoder().encode(value).length
  if (
    bytes === 0
    || bytes > 320
    || value.trim() !== value
    || hasControlCharacter(value)
    || [...value].some((character) => /\s/u.test(character))
  ) {
    return false
  }
  const parts = value.split('@')
  if (parts.length !== 2) return false
  const [local, domain] = parts
  return Boolean(
    local
    && domain
    && !local.startsWith('.')
    && !local.endsWith('.')
    && !domain.startsWith('.')
    && !domain.endsWith('.'),
  )
}

function isValidHost(value: string) {
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
  path: keyof EmailSettingsValues,
  message: string,
) {
  context.addIssue({ code: 'custom', path: [path], message })
}
