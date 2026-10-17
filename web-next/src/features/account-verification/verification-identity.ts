/** Names on the account page are masked; authorized reviewers retain the original. */
export function maskedVerificationName(name: string) {
  const characters = Array.from(name.trim())
  if (!characters.length) return '—'
  if (characters.length === 1) return '*'
  return characters[0] + '*'.repeat(Math.min(6, Math.max(1, characters.length - 2))) + (characters.length > 2 ? characters.at(-1) : '')
}
