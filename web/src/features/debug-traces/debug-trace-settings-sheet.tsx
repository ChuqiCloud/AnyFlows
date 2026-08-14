import { Check, LoaderCircle, Settings2, ShieldCheck } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from '@/components/ui/sheet'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { useAdminDebugTraceSettings, useUpdateAdminDebugTraceSettings } from './debug-trace-api'

const retentionOptions = [1, 6, 12, 24, 72, 168, 720] as const
const bodyLimitOptions = [4_096, 16_384, 32_768, 65_536] as const

/** 用易读百分比和保留期选项编辑底层百万分采样设置。 */
export function DebugTraceSettingsSheet() {
  const { t } = useTranslation()
  const query = useAdminDebugTraceSettings()
  const mutation = useUpdateAdminDebugTraceSettings()
  const [enabled, setEnabled] = useState(false)
  const [samplePercent, setSamplePercent] = useState('1')
  const [retentionHours, setRetentionHours] = useState('24')
  const [captureHeaders, setCaptureHeaders] = useState(false)
  const [captureBodies, setCaptureBodies] = useState(false)
  const [maxBodyBytes, setMaxBodyBytes] = useState('16384')

  useEffect(() => {
    if (!query.data) return
    setEnabled(query.data.enabled)
    setSamplePercent(formatPercent(query.data.sample_per_million))
    setRetentionHours(String(query.data.retention_hours))
    setCaptureHeaders(query.data.capture_headers)
    setCaptureBodies(query.data.capture_bodies)
    setMaxBodyBytes(String(query.data.max_body_bytes))
  }, [query.data])

  const samplePerMillion = parseSamplePercent(samplePercent)
  const invalid = samplePerMillion === undefined
  const dirty = useMemo(() => {
    if (!query.data || samplePerMillion === undefined) return false
    return enabled !== query.data.enabled
      || samplePerMillion !== query.data.sample_per_million
      || Number(retentionHours) !== query.data.retention_hours
      || captureHeaders !== query.data.capture_headers
      || captureBodies !== query.data.capture_bodies
      || Number(maxBodyBytes) !== query.data.max_body_bytes
  }, [captureBodies, captureHeaders, enabled, maxBodyBytes, query.data, retentionHours, samplePerMillion])

  const save = async () => {
    if (samplePerMillion === undefined) return
    try {
      await mutation.mutateAsync({
        enabled,
        sample_per_million: samplePerMillion,
        retention_hours: Number(retentionHours),
        capture_headers: captureHeaders,
        capture_bodies: captureBodies,
        max_body_bytes: Number(maxBodyBytes),
      })
    } catch {
      // 表单值保持不变，服务端诊断不进入浏览器展示。
    }
  }

  return (
    <Sheet>
      <SheetTrigger asChild>
        <Button type="button" size="sm" variant="secondary">
          <Settings2 aria-hidden="true" />
          {t('debugTraces.actions.settings')}
        </Button>
      </SheetTrigger>
      <SheetContent className="gap-0 overflow-hidden data-[side=right]:w-full data-[side=right]:sm:max-w-md">
        <SheetHeader className="shrink-0 border-b border-[var(--hairline)] pr-12">
          <div className="flex items-center gap-2">
            <SheetTitle>{t('debugTraces.settings.title')}</SheetTitle>
            {query.data ? <Badge>{t('debugTraces.settings.version', { version: query.data.version })}</Badge> : null}
          </div>
          <SheetDescription>{t('debugTraces.settings.description')}</SheetDescription>
        </SheetHeader>

        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain py-4">
          {query.isPending ? (
            <div className="grid gap-5 px-4" aria-label={t('debugTraces.settings.loading')}>
              {[0, 1, 2].map((item) => <Skeleton key={item} className="h-16 rounded-lg" />)}
            </div>
          ) : query.isError ? (
            <div className="mx-4 rounded-lg border border-destructive/25 bg-destructive/8 p-4" role="alert">
              <p className="text-sm font-semibold text-destructive">{t('debugTraces.settings.loadError')}</p>
              <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void query.refetch()}>
                {t('debugTraces.actions.retry')}
              </Button>
            </div>
          ) : (
            <div className="grid gap-5 px-4">
            <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
              <div>
                <Label htmlFor="debug-trace-enabled">{t('debugTraces.settings.enabled')}</Label>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('debugTraces.settings.enabledDescription')}</p>
              </div>
              <Switch id="debug-trace-enabled" checked={enabled} onCheckedChange={setEnabled} />
            </div>

            <div className="grid gap-2">
              <Label htmlFor="debug-trace-sample">{t('debugTraces.settings.sample')}</Label>
              <div className="relative">
                <Input
                  id="debug-trace-sample"
                  type="number"
                  min="0"
                  max="100"
                  step="0.01"
                  value={samplePercent}
                  aria-invalid={invalid}
                  className="pr-9 tabular-nums"
                  onChange={(event) => setSamplePercent(event.target.value)}
                />
                <span className="pointer-events-none absolute inset-y-0 right-3 grid place-items-center text-xs text-muted-foreground">%</span>
              </div>
              <p className={invalid ? 'text-xs text-destructive' : 'text-xs text-muted-foreground'}>
                {t(invalid ? 'debugTraces.settings.sampleInvalid' : 'debugTraces.settings.sampleHint')}
              </p>
            </div>

            <div className="grid gap-3 border-t border-[var(--hairline)] pt-5">
              <div>
                <h3 className="text-sm font-semibold">{t('debugTraces.settings.diagnostics')}</h3>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('debugTraces.settings.diagnosticsDescription')}</p>
              </div>

              <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
                <div>
                  <Label htmlFor="debug-trace-headers">{t('debugTraces.settings.captureHeaders')}</Label>
                  <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('debugTraces.settings.captureHeadersDescription')}</p>
                </div>
                <Switch id="debug-trace-headers" checked={captureHeaders} onCheckedChange={setCaptureHeaders} />
              </div>

              <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
                <div>
                  <Label htmlFor="debug-trace-bodies">{t('debugTraces.settings.captureBodies')}</Label>
                  <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('debugTraces.settings.captureBodiesDescription')}</p>
                </div>
                <Switch id="debug-trace-bodies" checked={captureBodies} onCheckedChange={setCaptureBodies} />
              </div>

              <div className="grid gap-2">
                <Label htmlFor="debug-trace-body-limit">{t('debugTraces.settings.bodyLimit')}</Label>
                <Select id="debug-trace-body-limit" value={maxBodyBytes} disabled={!captureBodies} onChange={(event) => setMaxBodyBytes(event.target.value)}>
                  {bodyLimitOptions.map((bytes) => (
                    <option key={bytes} value={bytes}>{t('debugTraces.settings.bodyLimitValue', { value: bytes / 1_024 })}</option>
                  ))}
                </Select>
              </div>
            </div>

            <div className="grid gap-2">
              <Label htmlFor="debug-trace-retention">{t('debugTraces.settings.retention')}</Label>
              <Select id="debug-trace-retention" value={retentionHours} onChange={(event) => setRetentionHours(event.target.value)}>
                {retentionOptions.map((hours) => (
                  <option key={hours} value={hours}>{t('debugTraces.settings.retentionValue', { hours })}</option>
                ))}
              </Select>
            </div>

            <div className="flex gap-3 rounded-lg border border-info/20 bg-info/8 p-3">
              <ShieldCheck className="mt-0.5 size-4 shrink-0 text-info" aria-hidden="true" />
              <p className="text-xs leading-5 text-muted-foreground">{t('debugTraces.settings.security')}</p>
            </div>
            </div>
          )}
        </div>

        <SheetFooter className="mt-0 shrink-0 border-t border-[var(--hairline)]">
          <div className="min-h-5 text-xs">
            {mutation.isError ? <span role="alert" className="text-destructive">{t('debugTraces.settings.saveError')}</span>
              : mutation.isSuccess && !dirty ? <span className="inline-flex items-center gap-1.5 text-success"><Check className="size-3.5" aria-hidden="true" />{t('debugTraces.settings.saved')}</span>
                : dirty ? <span className="text-muted-foreground">{t('debugTraces.settings.unsaved')}</span> : null}
          </div>
          <Button type="button" disabled={!dirty || invalid || mutation.isPending || query.isPending || query.isError} onClick={() => void save()}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
            {t(mutation.isPending ? 'debugTraces.actions.saving' : 'debugTraces.actions.save')}
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  )
}

function formatPercent(samplePerMillion: number) {
  return String(Number((samplePerMillion / 10_000).toFixed(2)))
}

function parseSamplePercent(value: string) {
  if (!/^\d{1,3}(?:\.\d{0,2})?$/.test(value)) return undefined
  const percent = Number(value)
  if (!Number.isFinite(percent) || percent < 0 || percent > 100) return undefined
  return Math.round(percent * 10_000)
}
