import { channelCapabilities, channelProtocols, runtimeChannelTypes } from './channel-capabilities.gen.ts'

export { channelCapabilities, channelProtocols, runtimeChannelTypes }

// Codex OAuth is a form preset stored as OpenAI Responses, not a transport adapter.
export const channelEditorTypes = ['codex_oauth', ...runtimeChannelTypes] as const
export type ChannelEditorType = (typeof channelEditorTypes)[number]
export type ChannelProtocol = (typeof channelProtocols)[number]
export type ChannelAdapterType = keyof typeof channelCapabilities

export function supportsProbe(type: ChannelAdapterType, protocol: string): boolean {
  return channelCapabilities[type].probeProtocols.some((supported) => supported === protocol)
}

export function supportsModelDiscovery(type: ChannelAdapterType, protocol: string): boolean {
  return channelCapabilities[type].modelDiscoveryProtocols.some((supported) => supported === protocol)
}

export function asChannelEditorType(value?: string): ChannelEditorType {
  return channelEditorTypes.find((type) => type === value) ?? 'openai'
}

export function protocolsForChannel(type: ChannelEditorType): readonly ChannelProtocol[] {
  return type === 'codex_oauth' ? ['openai_responses'] : channelCapabilities[type].protocols
}

/** Preserve a compatible selection when switching adapters; otherwise use its first protocol. */
export function selectChannelProtocol(type: ChannelEditorType, current?: string): ChannelProtocol {
  const protocols = protocolsForChannel(type)
  return protocols.find((protocol) => protocol === current) ?? protocols[0]
}
