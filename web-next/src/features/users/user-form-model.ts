import { z } from 'zod'

import type {
  AdminUser,
  AdminUserCreateRequest,
  AdminUserUpdateRequest,
} from '@/lib/api/generated/types.gen'

export type UserEditorMode = 'create' | 'update'

export type UserFormValues = {
  username: string
  email: string
  password: string
  role: 'user' | 'admin'
  status: 'enabled' | 'disabled'
  defaultGroupId: string
  quota: number
  rpmLimit: string
  concurrency: string
}

type ValidationMessages = {
  invalidUsername: string
  invalidEmail: string
  invalidPassword: string
  invalidGroup: string
  invalidNumber: string
}

const MAX_I32 = 2_147_483_647

/** 前端复现用户写入的公开边界，最终结果仍以后端校验为准。 */
export function buildUserFormSchema(mode: UserEditorMode, messages: ValidationMessages) {
  const quota = mode === 'create'
    ? z.number().int(messages.invalidNumber).safe(messages.invalidNumber).min(0, messages.invalidNumber)
    : z.number()
  return z.object({
    username: z.string().refine(isValidUsername, messages.invalidUsername),
    email: z.string().refine(isValidOptionalEmail, messages.invalidEmail),
    password: z.string().refine((value) => isValidPassword(value, mode), messages.invalidPassword),
    role: z.enum(['user', 'admin']),
    status: z.enum(['enabled', 'disabled']),
    defaultGroupId: z.string().refine((value) => parseInteger(value, 1) !== undefined, messages.invalidGroup),
    quota,
    rpmLimit: optionalNonNegativeInteger(messages.invalidNumber),
    concurrency: optionalNonNegativeInteger(messages.invalidNumber),
  })
}

export function defaultUserValues(user?: AdminUser): UserFormValues {
  return {
    username: user?.username ?? '',
    email: user?.email ?? '',
    password: '',
    role: user?.role ?? 'user',
    status: user?.status ?? 'enabled',
    defaultGroupId: user ? String(user.default_group_id) : '',
    quota: user?.quota ?? 0,
    rpmLimit: formatOptionalInteger(user?.rpm_limit),
    concurrency: formatOptionalInteger(user?.concurrency),
  }
}

export function toUserCreateRequest(values: UserFormValues): AdminUserCreateRequest {
  return {
    username: values.username,
    email: values.email || null,
    password: values.password || null,
    role: values.role,
    status: values.status,
    default_group_id: requireInteger(values.defaultGroupId, 1),
    quota: values.quota,
    rpm_limit: requireOptionalInteger(values.rpmLimit),
    concurrency: requireOptionalInteger(values.concurrency),
  }
}

/** 编辑用户只提交资料和限制字段，钱包余额必须通过增量调账修改。 */
export function toUserUpdateRequest(values: UserFormValues): AdminUserUpdateRequest {
  return {
    username: values.username,
    email: values.email || null,
    password: values.password || null,
    role: values.role,
    status: values.status,
    default_group_id: requireInteger(values.defaultGroupId, 1),
    rpm_limit: requireOptionalInteger(values.rpmLimit),
    concurrency: requireOptionalInteger(values.concurrency),
  }
}

/** 快速启停仍提交完整资料模型，但绝不直接覆盖钱包余额。 */
export function userRequestWithStatus(user: AdminUser, status: AdminUser['status']): AdminUserUpdateRequest {
  return {
    username: user.username,
    email: user.email,
    password: null,
    role: user.role,
    status,
    default_group_id: user.default_group_id,
    rpm_limit: user.rpm_limit,
    concurrency: user.concurrency,
  }
}

function optionalNonNegativeInteger(message: string) {
  return z.string().refine((value) => parseInteger(value, 0, true) !== undefined, message)
}

function parseInteger(value: string, minimum: number, optional = false): number | null | undefined {
  if (!value) return optional ? null : undefined
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) && parsed >= minimum && parsed <= MAX_I32 ? parsed : undefined
}

function requireInteger(value: string, minimum: number) {
  const parsed = parseInteger(value, minimum)
  if (typeof parsed !== 'number') throw new Error('用户表单整数未通过校验')
  return parsed
}

function requireOptionalInteger(value: string) {
  const parsed = parseInteger(value, 0, true)
  if (parsed === undefined) throw new Error('用户表单可选整数未通过校验')
  return parsed
}

function isValidUsername(value: string) {
  return value.length > 0
    && value.trim() === value
    && new TextEncoder().encode(value).length <= 64
    && ![...value].some((character) => /\p{Cc}/u.test(character))
}

function isValidOptionalEmail(value: string) {
  return value.length === 0 || (value.trim() === value
    && new TextEncoder().encode(value).length <= 320
    && ![...value].some((character) => /\p{Cc}/u.test(character)))
}

function isValidPassword(value: string, mode: UserEditorMode) {
  const size = new TextEncoder().encode(value).length
  return (mode === 'update' && size === 0) || (size > 0 && size <= 4096)
}

function formatOptionalInteger(value?: number | null) {
  return value === null || value === undefined ? '' : String(value)
}
