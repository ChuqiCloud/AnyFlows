import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import type { AdminDashboardChannelFlow, AdminDashboardResponse } from '@/lib/api/generated/types.gen'

type DashboardFlowMapProps = {
  dashboard: AdminDashboardResponse
  formatNumber: (value: number) => string
  formatPercent: (value: number) => string
}

type FlowNode = {
  key: string
  label: string
  total: number
  color: string
}

const SVG_WIDTH = 720
const NODE_WIDTH = 164
const NODE_HEIGHT = 32
const LEFT_X = 12
const RIGHT_X = SVG_WIDTH - NODE_WIDTH - 12
const TOP_PADDING = 18
const ROW_STEP = 48
const PALETTE = [
  'var(--chart-1)',
  'var(--chart-2)',
  'var(--chart-3)',
  'var(--chart-4)',
  'var(--chart-5)',
]

function shortenLabel(value: string, limit: number) {
  if (value.length <= limit)
    return value
  return `${value.slice(0, limit - 1)}…`
}

function protocolLabel(protocol: string, translate: (key: string) => string) {
  const translated = translate(`dashboard.outcomes.protocols.${protocol}`)
  return translated === `dashboard.outcomes.protocols.${protocol}` ? protocol : translated
}

function buildNodes(flows: AdminDashboardChannelFlow[], translate: (key: string) => string) {
  const protocolTotals = new Map<string, number>()
  const channelTotals = new Map<string, { label: string; total: number }>()

  for (const flow of flows) {
    protocolTotals.set(flow.protocol, (protocolTotals.get(flow.protocol) ?? 0) + flow.request_count)
    const channelKey = String(flow.channel_id)
    const channel = channelTotals.get(channelKey)
    channelTotals.set(channelKey, {
      label: channel?.label ?? flow.channel_name,
      total: (channel?.total ?? 0) + flow.request_count,
    })
  }

  const protocols: FlowNode[] = [...protocolTotals.entries()].map(([key, total], index) => ({
    key,
    label: protocolLabel(key, translate),
    total,
    color: PALETTE[index % PALETTE.length],
  }))
  const channels: FlowNode[] = [...channelTotals.entries()].map(([key, channel], index) => ({
    key,
    label: channel.label,
    total: channel.total,
    color: PALETTE[(index + 2) % PALETTE.length],
  }))

  return { protocols, channels }
}

function nodeY(index: number) {
  return TOP_PADDING + index * ROW_STEP
}

function flowPath(leftY: number, rightY: number) {
  const startX = LEFT_X + NODE_WIDTH
  const endX = RIGHT_X
  const controlOffset = (endX - startX) * 0.48
  return `M ${startX} ${leftY + NODE_HEIGHT / 2} C ${startX + controlOffset} ${leftY + NODE_HEIGHT / 2}, ${endX - controlOffset} ${rightY + NODE_HEIGHT / 2}, ${endX} ${rightY + NODE_HEIGHT / 2}`
}

/** 用真实终态事实绘制入口协议到最终渠道的聚合流向。 */
export function DashboardFlowMap({ dashboard, formatNumber, formatPercent }: DashboardFlowMapProps) {
  const { t } = useTranslation()
  const { protocols, channels } = useMemo(
    () => buildNodes(dashboard.channel_flows, t),
    [dashboard.channel_flows, t],
  )
  const protocolIndex = new Map(protocols.map((node, index) => [node.key, index]))
  const channelIndex = new Map(channels.map((node, index) => [node.key, index]))
  const maximumFlow = Math.max(0, ...dashboard.channel_flows.map(flow => flow.request_count))
  const chartRows = Math.max(protocols.length, channels.length, 1)
  const chartHeight = TOP_PADDING * 2 + NODE_HEIGHT + (chartRows - 1) * ROW_STEP
  const visibleTotal = dashboard.channel_flows.reduce((total, flow) => total + flow.request_count, 0)
  const successTotal = dashboard.successful_request_count
  const visibleShare = successTotal > 0 ? visibleTotal / successTotal : null

  if (dashboard.channel_flows.length === 0) {
    return <div className="py-8 text-center text-xs text-muted-foreground">{t('dashboard.outcomes.noFlows')}</div>
  }

  return (
    <div className="grid gap-3 px-4 py-3">
      <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1 text-[0.6875rem] text-muted-foreground">
        <span>{t('dashboard.outcomes.flowMap.visible', { count: formatNumber(visibleTotal) })}</span>
        <span>{visibleShare === null ? '--' : t('dashboard.outcomes.flowMap.share', { rate: formatPercent(visibleShare) })}</span>
      </div>
      <div className="overflow-x-auto rounded-lg border border-[var(--hairline)] bg-surface-2/25">
        <svg
          className="h-auto min-h-56 min-w-[560px] w-full"
          viewBox={`0 0 ${SVG_WIDTH} ${chartHeight}`}
          role="img"
          aria-label={t('dashboard.outcomes.flowMap.chartLabel')}
        >
          <title>{t('dashboard.outcomes.flowMap.chartTitle')}</title>
          <desc>{t('dashboard.outcomes.flowMap.chartDescription')}</desc>
          <g fill="none" strokeLinecap="round">
            {dashboard.channel_flows.map((flow) => {
              const left = protocolIndex.get(flow.protocol)
              const right = channelIndex.get(String(flow.channel_id))
              if (left === undefined || right === undefined)
                return null
              const opacity = 0.24 + (flow.request_count / maximumFlow) * 0.56
              const strokeWidth = Math.max(2, (flow.request_count / maximumFlow) * 16)
              return (
                <path
                  key={`${flow.protocol}-${flow.channel_id}`}
                  d={flowPath(nodeY(left), nodeY(right))}
                  stroke={protocols[left].color}
                  strokeOpacity={opacity}
                  strokeWidth={strokeWidth}
                >
                  <title>{t('dashboard.outcomes.flowMap.path', {
                    protocol: protocols[left].label,
                    channel: channels[right].label,
                    count: formatNumber(flow.request_count),
                  })}</title>
                </path>
              )
            })}
          </g>
          <g>
            {protocols.map((node, index) => (
              <g key={node.key}>
                <rect x={LEFT_X} y={nodeY(index)} width={NODE_WIDTH} height={NODE_HEIGHT} rx="7" fill={node.color} fillOpacity="0.14" stroke={node.color} strokeOpacity="0.45" />
                <text x={LEFT_X + 10} y={nodeY(index) + 14} fill="currentColor" fontSize="12" fontWeight="600">
                  {shortenLabel(node.label, 21)}
                </text>
                <text x={LEFT_X + 10} y={nodeY(index) + 26} fill="currentColor" fillOpacity="0.58" fontSize="10">
                  {formatNumber(node.total)} {t('dashboard.outcomes.flowMap.requests')}
                </text>
                <title>{`${node.label}: ${formatNumber(node.total)}`}</title>
              </g>
            ))}
            {channels.map((node, index) => (
              <g key={node.key}>
                <rect x={RIGHT_X} y={nodeY(index)} width={NODE_WIDTH} height={NODE_HEIGHT} rx="7" fill={node.color} fillOpacity="0.14" stroke={node.color} strokeOpacity="0.45" />
                <text x={RIGHT_X + 10} y={nodeY(index) + 14} fill="currentColor" fontSize="12" fontWeight="600">
                  {shortenLabel(node.label, 21)}
                </text>
                <text x={RIGHT_X + 10} y={nodeY(index) + 26} fill="currentColor" fillOpacity="0.58" fontSize="10">
                  {formatNumber(node.total)} {t('dashboard.outcomes.flowMap.requests')}
                </text>
                <title>{`${node.label}: ${formatNumber(node.total)}`}</title>
              </g>
            ))}
          </g>
        </svg>
      </div>
      {dashboard.other_success_count > 0 ? (
        <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
          <span>{t('dashboard.outcomes.otherFlows')}</span>
          <span className="tabular-nums">{formatNumber(dashboard.other_success_count)}</span>
        </div>
      ) : null}
    </div>
  )
}
