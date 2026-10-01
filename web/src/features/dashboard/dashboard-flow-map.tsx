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
type FlowNode = { key: string, label: string, total: number, color: string, layer: number, isOther: boolean }
type FlowEdge = { key: string, source: string, target: string, total: number, color: string, layer: number }
type FlowLayer = { key: string, nodes: FlowNode[] }

const SVG_WIDTH = 1160
const NODE_WIDTH = 12
const NODE_HEIGHT = 42
const TOP_PADDING = 42
const ROW_STEP = 62
const LAYER_X = [128, 430, 730, 1030]
const TOP_LIMIT_OPTIONS = [6, 10, 14] as const
const PALETTE = [
  'var(--chart-1)',
  'var(--chart-2)',
  'var(--chart-3)',
  'var(--chart-4)',
]

function shortenLabel(value: string, limit = 21) {
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

function buildFlow(
  paths: AdminDashboardFlowPath[],
  mode: FlowMode,
  userLabel: (id: number) => string,
  otherLabel: string,
  topLimit: number,
) {
  const nodeMaps = Array.from({ length: 4 }, () => new Map<string, { label: string, total: number, isOther: boolean }>())
  const edgeMaps = Array.from({ length: 3 }, () => new Map<string, FlowEdge>())
  const rawTotals = Array.from({ length: 4 }, () => new Map<string, number>())

  for (const path of paths) {
    const amount = mode === 'requests' ? path.request_count : path.quota_consumed
    if (amount <= 0) continue
    const keys = pathKeys(path)
    keys.forEach((key, layer) => rawTotals[layer].set(key, (rawTotals[layer].get(key) ?? 0) + amount))
  }

  const visibleKeys = rawTotals.map(totals => new Set(
    [...totals.entries()]
      .sort((left, right) => right[1] - left[1] || left[0].localeCompare(right[0]))
      .slice(0, topLimit)
      .map(([key]) => key),
  ))

  for (const path of paths) {
    const amount = mode === 'requests' ? path.request_count : path.quota_consumed
    if (amount <= 0) continue
    const rawKeys = pathKeys(path)
    const labels = pathLabels(path, userLabel)
    const keys = rawKeys.map((key, layer) => visibleKeys[layer].has(key) ? key : `other:${layer}`)
    const resolvedLabels = keys.map((key, layer) => key.startsWith('other:') ? otherLabel : labels[layer])

    for (let layer = 0; layer < keys.length; layer += 1) {
      const key = keys[layer]
      const current = nodeMaps[layer].get(key)
      nodeMaps[layer].set(key, {
        label: current?.label ?? resolvedLabels[layer],
        total: (current?.total ?? 0) + amount,
        isOther: key.startsWith('other:'),
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
        color: PALETTE[layer],
        layer,
      })
    }
  }

  const layerKeys = ['users', 'groups', 'channels', 'models']
  const layers: FlowLayer[] = nodeMaps.map((nodes, layer) => ({
    key: layerKeys[layer],
    nodes: [...nodes.entries()]
      .map(([key, node]) => ({
        key,
        label: node.label,
        total: node.total,
        color: PALETTE[layer],
        layer,
        isOther: node.isOther,
      }))
      .sort((left, right) => Number(left.isOther) - Number(right.isOther) || right.total - left.total || left.label.localeCompare(right.label)),
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
  const [topLimit, setTopLimit] = useState<number>(10)
  const [activeKey, setActiveKey] = useState<string>()
  const { layers, edges } = useMemo(
    () => buildFlow(dashboard.flow_paths, mode, id => t('dashboard.outcomes.flowMap.user', { id }), t('dashboard.outcomes.flowMap.other'), topLimit),
    [dashboard.flow_paths, mode, t, topLimit],
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
  const chartHeight = TOP_PADDING * 2 + NODE_HEIGHT + (chartRows - 1) * ROW_STEP + 12
  const activeNode = activeKey ? layers.flatMap(layer => layer.nodes).find(node => node.key === activeKey) : undefined
  const relatedKeys = useMemo(() => {
    if (!activeKey) return undefined
    const keys = new Set([activeKey])
    let changed = true
    while (changed) {
      changed = false
      for (const edge of edges.flat()) {
        if (keys.has(edge.source) || keys.has(edge.target)) {
          const size = keys.size
          keys.add(edge.source)
          keys.add(edge.target)
          changed = changed || keys.size !== size
        }
      }
    }
    return keys
  }, [activeKey, edges])

  if (dashboard.flow_paths.length === 0) {
    return <div className="py-8 text-center text-xs text-muted-foreground">{t('dashboard.outcomes.flowMap.empty')}</div>
  }

  return (
    <div className="grid gap-4 px-4 py-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="grid gap-2">
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-[0.6875rem] text-muted-foreground">
            <span>{t('dashboard.outcomes.flowMap.visible', { value: formatValue(visible), rate: visibleShare === null ? '--' : formatPercent(visibleShare) })}</span>
            <span className="inline-flex items-center gap-1.5"><i className="size-1.5 rounded-full bg-[var(--chart-1)]" />{t('dashboard.outcomes.flowMap.highlightHint')}</span>
          </div>
          <div className="flex flex-wrap items-center gap-2 text-[0.6875rem] text-muted-foreground">
            <span>{t('dashboard.outcomes.flowMap.topLimit')}</span>
            <div className="inline-flex rounded-md border border-[var(--hairline)] bg-surface-2 p-0.5" role="group" aria-label={t('dashboard.outcomes.flowMap.topLimit')}>
              {TOP_LIMIT_OPTIONS.map(value => (
                <Button
                  key={value}
                  type="button"
                  size="sm"
                  variant={topLimit === value ? 'secondary' : 'ghost'}
                  className="h-7 px-2 text-[0.6875rem]"
                  aria-pressed={topLimit === value}
                  onClick={() => setTopLimit(value)}
                >
                  {t(`dashboard.outcomes.flowMap.topOptions.${value}`)}
                </Button>
              ))}
            </div>
          </div>
        </div>
        <div className="inline-flex rounded-lg border border-[var(--hairline)] bg-surface-2 p-0.5" role="group" aria-label={t('dashboard.outcomes.flowMap.modeLabel')}>
          {(['requests', 'quota'] as const).map(value => (
            <Button
              key={value}
              type="button"
              size="sm"
              variant={mode === value ? 'secondary' : 'ghost'}
              className="h-8 px-3 text-xs"
              aria-pressed={mode === value}
              onClick={() => setMode(value)}
            >
              {t(`dashboard.outcomes.flowMap.modes.${value}`)}
            </Button>
          ))}
        </div>
      </div>

      <div className="report-flow-frame overflow-x-auto rounded-xl border border-[var(--hairline)] bg-surface-2/25">
        <svg className="h-auto min-h-72 min-w-[940px] w-full" viewBox={`0 0 ${SVG_WIDTH} ${chartHeight}`} role="img" aria-label={t('dashboard.outcomes.flowMap.chartLabel')}>
          <title>{t('dashboard.outcomes.flowMap.chartTitle')}</title>
          <desc>{t('dashboard.outcomes.flowMap.chartDescription')}</desc>
          <defs>
            <pattern id="flow-grid" width="24" height="24" patternUnits="userSpaceOnUse">
              <path d="M 24 0 L 0 0 0 24" fill="none" stroke="currentColor" strokeOpacity="0.045" strokeWidth="1" />
            </pattern>
            {edges.map((_, layer) => (
              <linearGradient key={`flow-gradient-${layer}`} id={`flow-gradient-${layer}`} x1="0" x2="1" y1="0" y2="0">
                <stop offset="0" stopColor={PALETTE[layer]} stopOpacity="0.24" />
                <stop offset="0.5" stopColor={PALETTE[layer]} stopOpacity="0.72" />
                <stop offset="1" stopColor={PALETTE[(layer + 1) % PALETTE.length]} stopOpacity="0.32" />
              </linearGradient>
            ))}
          </defs>
          <rect width={SVG_WIDTH} height={chartHeight} fill="url(#flow-grid)" />
          <g fill="none" strokeLinecap="round">
            {edges.flatMap((layerEdges, layer) => layerEdges.map((edge) => {
              const sourceIndex = nodeIndexes[layer].get(edge.source)
              const targetIndex = nodeIndexes[layer + 1].get(edge.target)
              if (sourceIndex === undefined || targetIndex === undefined || edge.total <= 0 || maximumEdge <= 0) return null
              const related = !relatedKeys || relatedKeys.has(edge.source) || relatedKeys.has(edge.target)
              const width = Math.max(2, Math.sqrt(edge.total / maximumEdge) * 30)
              return (
                <path
                  key={`${layer}:${edge.key}`}
                  d={edgePath(LAYER_X[layer] + NODE_WIDTH, nodeY(sourceIndex) + NODE_HEIGHT / 2, LAYER_X[layer + 1], nodeY(targetIndex) + NODE_HEIGHT / 2)}
                  stroke={`url(#flow-gradient-${layer})`}
                  strokeOpacity={related ? 1 : 0.08}
                  strokeWidth={width}
                  className="report-flow-edge"
                >
                  <title>{t('dashboard.outcomes.flowMap.edge', { value: formatValue(edge.total) })}</title>
                </path>
              )
            }))}
          </g>
          {layers.map((layer, layerIndex) => (
            <g key={layer.key}>
              <g transform={`translate(${LAYER_X[layerIndex]}, 17)`}>
                <circle r="4" fill={PALETTE[layerIndex]} />
                <text x="10" y="4" fill="currentColor" fillOpacity="0.68" fontSize="11" fontWeight="650">{t(`dashboard.outcomes.flowMap.layers.${layer.key}`)}</text>
                <text x="10" y="18" fill="currentColor" fillOpacity="0.42" fontSize="9">{formatValue(layer.nodes.reduce((sum, node) => sum + node.total, 0))}</text>
              </g>
              {layer.nodes.map((node, index) => {
                const y = nodeY(index)
                const related = !relatedKeys || relatedKeys.has(node.key)
                const labelX = layerIndex === 0 ? LAYER_X[layerIndex] - 12 : LAYER_X[layerIndex] + NODE_WIDTH + 12
                const anchor = layerIndex === 0 ? 'end' : 'start'
                return (
                  <g
                    key={node.key}
                    className="report-flow-node"
                    opacity={related ? 1 : 0.28}
                    role="button"
                    tabIndex={0}
                    aria-label={`${node.label}: ${formatValue(node.total)}`}
                    onMouseEnter={() => setActiveKey(node.key)}
                    onMouseLeave={() => setActiveKey(undefined)}
                    onFocus={() => setActiveKey(node.key)}
                    onBlur={() => setActiveKey(undefined)}
                  >
                    <rect x={LAYER_X[layerIndex] - 2} y={y - 4} width={NODE_WIDTH + 4} height={NODE_HEIGHT + 8} rx="7" fill={node.color} fillOpacity="0.1" />
                    <rect x={LAYER_X[layerIndex]} y={y} width={NODE_WIDTH} height={NODE_HEIGHT} rx="5" fill={node.color} fillOpacity={activeNode?.key === node.key ? '1' : '0.78'} stroke={node.color} strokeOpacity="0.9" />
                    <text x={labelX} y={y + 14} textAnchor={anchor} fill="currentColor" fontSize="11" fontWeight="650">{shortenLabel(node.label)}</text>
                    <text x={labelX} y={y + 29} textAnchor={anchor} fill="currentColor" fillOpacity="0.55" fontSize="9">{formatValue(node.total)}</text>
                    <title>{`${node.label}: ${formatValue(node.total)}`}</title>
                  </g>
                )
              })}
            </g>
          ))}
        </svg>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-3 text-[0.6875rem] text-muted-foreground">
        <div className="flex flex-wrap items-center gap-3">
          {layers.map((layer, index) => <span key={layer.key} className="inline-flex items-center gap-1.5"><i className="size-1.5 rounded-full" style={{ background: PALETTE[index] }} />{t(`dashboard.outcomes.flowMap.layers.${layer.key}`)}</span>)}
        </div>
        {other > 0 ? <span className="tabular-nums">{t('dashboard.outcomes.flowMap.other')}: {formatValue(other)}</span> : null}
      </div>
    </div>
  )
}
