import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import type { AdminDashboardFlowPath, AdminDashboardResponse } from '@/lib/api/generated/types.gen'

type DashboardFlowMapProps = {
  dashboard: AdminDashboardResponse
  formatNumber: (value: number) => string
  formatPercent: (value: number) => string
  formatQuota: (value: number) => string
}

type FlowMode = 'requests' | 'quota'
type FlowNode = { key: string, label: string, total: number, color: string }
type FlowEdge = { key: string, source: string, target: string, total: number, color: string }
type FlowLayer = { key: string, nodes: FlowNode[] }

const SVG_WIDTH = 1120
const NODE_WIDTH = 190
const NODE_HEIGHT = 34
const TOP_PADDING = 20
const ROW_STEP = 48
const LAYER_X = [12, 310, 608, 906]
const PALETTE = [
  'var(--chart-1)',
  'var(--chart-2)',
  'var(--chart-3)',
  'var(--chart-4)',
  'var(--chart-5)',
]

function shortenLabel(value: string, limit = 24) {
  return value.length <= limit ? value : `${value.slice(0, limit - 1)}…`
}

function pathKeys(path: AdminDashboardFlowPath) {
  return [
    `user:${path.user_id}`,
    `group:${path.group_id}`,
    `channel:${path.channel_id}`,
    `model:${path.model}`,
  ]
}

function pathLabels(path: AdminDashboardFlowPath, userLabel: (id: number) => string) {
  return [userLabel(path.user_id), path.group_name, path.channel_name, path.model]
}

function buildFlow(paths: AdminDashboardFlowPath[], mode: FlowMode, userLabel: (id: number) => string) {
  const nodeMaps = Array.from({ length: 4 }, () => new Map<string, { label: string, total: number }>())
  const edgeMaps = Array.from({ length: 3 }, () => new Map<string, FlowEdge>())

  for (const path of paths) {
    const amount = mode === 'requests' ? path.request_count : path.quota_consumed
    const keys = pathKeys(path)
    const labels = pathLabels(path, userLabel)
    for (let layer = 0; layer < keys.length; layer += 1) {
      const current = nodeMaps[layer].get(keys[layer])
      nodeMaps[layer].set(keys[layer], {
        label: current?.label ?? labels[layer],
        total: (current?.total ?? 0) + amount,
      })
    }
    for (let layer = 0; layer < keys.length - 1; layer += 1) {
      const edgeKey = `${keys[layer]}|${keys[layer + 1]}`
      const current = edgeMaps[layer].get(edgeKey)
      edgeMaps[layer].set(edgeKey, {
        key: edgeKey,
        source: keys[layer],
        target: keys[layer + 1],
        total: (current?.total ?? 0) + amount,
        color: PALETTE[layer % PALETTE.length],
      })
    }
  }

  const layerKeys = ['users', 'groups', 'channels', 'models']
  const layers: FlowLayer[] = nodeMaps.map((nodes, layer) => ({
    key: layerKeys[layer],
    nodes: [...nodes.entries()]
      .map(([key, node], index) => ({
        key,
        label: node.label,
        total: node.total,
        color: PALETTE[(index + layer) % PALETTE.length],
      }))
      .sort((left, right) => right.total - left.total || left.label.localeCompare(right.label)),
  }))

  return { layers, edges: edgeMaps.map(edges => [...edges.values()]) }
}

function nodeY(index: number) {
  return TOP_PADDING + index * ROW_STEP
}

function edgePath(sourceX: number, sourceY: number, targetX: number, targetY: number) {
  const controlOffset = (targetX - sourceX) * 0.48
  return `M ${sourceX} ${sourceY} C ${sourceX + controlOffset} ${sourceY}, ${targetX - controlOffset} ${targetY}, ${targetX} ${targetY}`
}

/** 用低敏已结算事实绘制用户、分组、渠道与模型四层流向。 */
export function DashboardFlowMap({ dashboard, formatNumber, formatPercent, formatQuota }: DashboardFlowMapProps) {
  const { t } = useTranslation()
  const [mode, setMode] = useState<FlowMode>('requests')
  const { layers, edges } = useMemo(
    () => buildFlow(dashboard.flow_paths, mode, id => t('dashboard.outcomes.flowMap.user', { id })),
    [dashboard.flow_paths, mode, t],
  )
  const formatValue = mode === 'requests' ? formatNumber : formatQuota
  const total = mode === 'requests' ? dashboard.flow_request_count : dashboard.flow_quota_consumed
  const visible = dashboard.flow_paths.reduce(
    (sum, path) => sum + (mode === 'requests' ? path.request_count : path.quota_consumed),
    0,
  )
  const other = Math.max(0, total - visible)
  const visibleShare = total > 0 ? visible / total : null
  const nodeIndexes = layers.map(layer => new Map(layer.nodes.map((node, index) => [node.key, index])))
  const maximumEdge = Math.max(0, ...edges.flat().map(edge => edge.total))
  const chartRows = Math.max(1, ...layers.map(layer => layer.nodes.length))
  const chartHeight = TOP_PADDING * 2 + NODE_HEIGHT + (chartRows - 1) * ROW_STEP

  if (dashboard.flow_paths.length === 0) {
    return <div className="py-8 text-center text-xs text-muted-foreground">{t('dashboard.outcomes.flowMap.empty')}</div>
  }

  return (
    <div className="grid gap-3 px-4 py-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="text-[0.6875rem] text-muted-foreground">
          {t('dashboard.outcomes.flowMap.visible', {
            value: formatValue(visible),
            rate: visibleShare === null ? '--' : formatPercent(visibleShare),
          })}
        </div>
        <div className="inline-flex rounded-md border border-[var(--hairline)] bg-surface-2 p-0.5" role="group" aria-label={t('dashboard.outcomes.flowMap.modeLabel')}>
          {(['requests', 'quota'] as const).map(value => (
            <Button
              key={value}
              type="button"
              size="sm"
              variant={mode === value ? 'secondary' : 'ghost'}
              className="h-7 px-2.5 text-xs"
              aria-pressed={mode === value}
              onClick={() => setMode(value)}
            >
              {t(`dashboard.outcomes.flowMap.modes.${value}`)}
            </Button>
          ))}
        </div>
      </div>
      <div className="overflow-x-auto rounded-lg border border-[var(--hairline)] bg-surface-2/25">
        <svg className="h-auto min-h-64 min-w-[880px] w-full" viewBox={`0 0 ${SVG_WIDTH} ${chartHeight}`} role="img" aria-label={t('dashboard.outcomes.flowMap.chartLabel')}>
          <title>{t('dashboard.outcomes.flowMap.chartTitle')}</title>
          <desc>{t('dashboard.outcomes.flowMap.chartDescription')}</desc>
          <g fill="none" strokeLinecap="round">
            {edges.flatMap((layerEdges, layer) => layerEdges.map((edge) => {
              const sourceIndex = nodeIndexes[layer].get(edge.source)
              const targetIndex = nodeIndexes[layer + 1].get(edge.target)
              if (sourceIndex === undefined || targetIndex === undefined || edge.total <= 0 || maximumEdge <= 0)
                return null
              return (
                <path
                  key={`${layer}:${edge.key}`}
                  d={edgePath(LAYER_X[layer] + NODE_WIDTH, nodeY(sourceIndex) + NODE_HEIGHT / 2, LAYER_X[layer + 1], nodeY(targetIndex) + NODE_HEIGHT / 2)}
                  stroke={edge.color}
                  strokeOpacity={0.24 + (edge.total / maximumEdge) * 0.46}
                  strokeWidth={Math.max(1.5, (edge.total / maximumEdge) * 18)}
                >
                  <title>{t('dashboard.outcomes.flowMap.edge', { value: formatValue(edge.total) })}</title>
                </path>
              )
            }))}
          </g>
          {layers.map((layer, layerIndex) => (
            <g key={layer.key}>
              <text x={LAYER_X[layerIndex]} y={13} fill="currentColor" fillOpacity="0.58" fontSize="10" fontWeight="600">
                {t(`dashboard.outcomes.flowMap.layers.${layer.key}`)}
              </text>
              {layer.nodes.map((node, index) => (
                <g key={node.key}>
                  <rect x={LAYER_X[layerIndex]} y={nodeY(index)} width={NODE_WIDTH} height={NODE_HEIGHT} rx="6" fill={node.color} fillOpacity="0.13" stroke={node.color} strokeOpacity="0.45" />
                  <text x={LAYER_X[layerIndex] + 10} y={nodeY(index) + 15} fill="currentColor" fontSize="12" fontWeight="600">{shortenLabel(node.label)}</text>
                  <text x={LAYER_X[layerIndex] + 10} y={nodeY(index) + 28} fill="currentColor" fillOpacity="0.58" fontSize="10">{formatValue(node.total)}</text>
                  <title>{`${node.label}: ${formatValue(node.total)}`}</title>
                </g>
              ))}
            </g>
          ))}
        </svg>
      </div>
      {other > 0 ? (
        <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
          <span>{t('dashboard.outcomes.flowMap.other')}</span>
          <span className="tabular-nums">{formatValue(other)}</span>
        </div>
      ) : null}
    </div>
  )
}
