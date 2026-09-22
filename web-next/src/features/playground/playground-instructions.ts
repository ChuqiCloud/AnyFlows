/** 试炼场不注入默认系统提示词，只有用户主动填写的非空内容才进入请求。 */
export function normalizePlaygroundInstructions(value: string) {
  const instructions = value.trim()
  return instructions || undefined
}
