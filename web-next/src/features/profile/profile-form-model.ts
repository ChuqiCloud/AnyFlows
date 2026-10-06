const MAX_USERNAME_BYTES = 64
const MIN_PASSWORD_BYTES = 12
const MAX_PASSWORD_BYTES = 128

/** 复现服务端用户名边界，最终结果仍以后端校验为准。 */
export function isValidProfileUsername(value: string) {
  return value.length > 0
    && value.trim() === value
    && new TextEncoder().encode(value).length <= MAX_USERNAME_BYTES
    && ![...value].some((character) => /\p{Cc}/u.test(character))
}

/** 复现服务端新密码边界，避免在提交前暴露无效表单。 */
export function isValidProfilePassword(value: string) {
  const bytes = new TextEncoder().encode(value).length
  return bytes >= MIN_PASSWORD_BYTES
    && bytes <= MAX_PASSWORD_BYTES
    && ![...value].some((character) => /\p{Cc}/u.test(character))
}
