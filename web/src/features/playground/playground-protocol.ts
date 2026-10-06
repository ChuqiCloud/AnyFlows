import type {
  ModelCatalogItem,
  ModelCatalogProtocol,
} from '@/lib/api/generated/types.gen'

/** 试炼场当前可以直接发起请求的客户端协议。 */
export type PlaygroundProtocol = Extract<
  ModelCatalogProtocol,
  'openai_chat' | 'openai_responses' | 'anthropic'
>

export type PlaygroundModelTarget = {
  model: string
  protocol: PlaygroundProtocol
}

/**
 * 按兼容性优先级选择试炼场入口；只有 Responses 时才使用 Responses。
 * 其它上游协议必须通过对应的客户端入口，不能在这里隐式桥接。
 */
export function preferredPlaygroundProtocol(
  protocols: readonly ModelCatalogProtocol[],
): PlaygroundProtocol | undefined {
  if (protocols.includes('openai_chat')) return 'openai_chat'
  if (protocols.includes('openai_responses')) return 'openai_responses'
  if (protocols.includes('anthropic')) return 'anthropic'
  return undefined
}

/** 从权威模型目录项读取试炼场可用入口。 */
export function playgroundProtocolForModel(item: ModelCatalogItem) {
  return preferredPlaygroundProtocol(item.available_protocols ?? [])
}

/** 校验当前选中的每个模型都有已知的可调用协议。 */
export function resolvePlaygroundTargets(
  models: readonly string[],
  protocolByModel: Readonly<Record<string, PlaygroundProtocol>>,
): PlaygroundModelTarget[] | undefined {
  const targets: PlaygroundModelTarget[] = []
  for (const model of models) {
    const protocol = protocolByModel[model]
    if (!protocol) return undefined
    targets.push({ model, protocol })
  }
  return targets
}
