/** 平台理由必须保持原样进入审计，前端只做与服务端一致的即时校验。 */
export function isValidPlatformOrganizationReason(reason: string) {
  return reason.length > 0
    && reason.trim() === reason
    && !Array.from(reason).some((character) => /\p{Cc}/u.test(character))
    && new TextEncoder().encode(reason).length <= 255
}
