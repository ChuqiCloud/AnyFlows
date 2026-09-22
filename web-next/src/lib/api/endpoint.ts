import { apiUrl as resolveApiUrl, resolveApiBaseUrl } from '@/shared/utils/env'

/**
 * 后端基址统一由平台层 `@/shared/utils/env` 定义，业务层只做转发，
 * 保证平台 HTTP 客户端与业务生成客户端的寻址规则始终一致。
 */
export const apiBaseUrl = resolveApiBaseUrl()

/** 拼接后端请求地址；基址为空时返回同源相对路径。 */
export const apiUrl = resolveApiUrl
