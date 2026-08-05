import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { AdminChannel, AdminRoute } from '@/lib/api/generated/types.gen'
import { RouteCandidateEditor } from './route-candidate-editor'
import { routeWriteErrorCode, useCreateAdminRoute, useUpdateAdminRoute } from './route-api'
import { RouteMappingEditor } from './route-mapping-editor'
import {
  defaultRouteFormValues,
  toRouteWriteRequest,
  validateRouteForm,
  type RouteFormValues,
  type RouteFormErrors,
} from './route-form-model'

type RouteFormProps = {
  mode: 'create' | 'update'
  route?: AdminRoute
  channels: readonly AdminChannel[]
  candidatesLoading: boolean
  candidatesError: boolean
  onRetryCandidates: () => void
  onCancel: () => void
  onSaved: (route: AdminRoute) => void
}

/** 智能路由的结构化编辑表单，提交前将拖拽顺序转换为服务端优先级。 */
export function RouteForm({
  mode,
  route,
  channels,
  candidatesLoading,
  candidatesError,
  onRetryCandidates,
  onCancel,
  onSaved,
}: RouteFormProps) {
  const { t } = useTranslation()
  const createMutation = useCreateAdminRoute()
  const updateMutation = useUpdateAdminRoute()
  const [values, setValues] = useState<RouteFormValues>(() => defaultRouteFormValues(route))
  const [errors, setErrors] = useState<RouteFormErrors>({})

  useEffect(() => {
    setValues(defaultRouteFormValues(route))
    setErrors({})
  }, [mode, route])

  const pending = createMutation.isPending || updateMutation.isPending
  const submitError = createMutation.error ?? updateMutation.error

  const updateValues = (patch: Partial<RouteFormValues>) => {
    setValues((current) => ({ ...current, ...patch }))
    setErrors({})
  }

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const nextErrors = validateRouteForm(values)
    setErrors(nextErrors)
    if (Object.keys(nextErrors).length > 0) return

    const body = toRouteWriteRequest(values)
    try {
      const saved = mode === 'create'
        ? await createMutation.mutateAsync(body)
        : route
          ? await updateMutation.mutateAsync({ id: route.id, body })
          : undefined
      if (saved) onSaved(saved)
    } catch {
      // 保存失败时保留表单状态，便于管理员修正候选引用后重试。
    }
  }

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit} noValidate>
      <div className="flex-1 space-y-6 overflow-y-auto px-4 py-5">
        <section className="grid gap-3" aria-labelledby="route-identity-fields">
          <h3 id="route-identity-fields" className="text-xs font-semibold">{t('routes.form.identity')}</h3>
          <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start">
            <div className="grid gap-1.5">
              <Label htmlFor="route-name">{t('routes.form.name')}</Label>
              <Input id="route-name" autoComplete="off" value={values.name} aria-invalid={Boolean(errors.name)} onChange={(event) => updateValues({ name: event.target.value })} />
              {errors.name ? <p className="text-xs text-destructive">{t(`routes.validation.${errors.name}`)}</p> : null}
            </div>
            <div className="flex items-center gap-3 rounded-lg border border-[var(--hairline)] px-3 py-2.5 sm:mt-6">
              <div className="min-w-0"><p className="text-xs font-medium">{t('routes.form.enabled')}</p><p className="mt-0.5 text-[0.6875rem] text-muted-foreground">{t('routes.form.enabledHint')}</p></div>
              <Switch checked={values.enabled} aria-label={t('routes.form.enabled')} onCheckedChange={(enabled) => updateValues({ enabled })} />
            </div>
          </div>
        </section>

        <section className="grid gap-3" aria-labelledby="route-matching-fields">
          <h3 id="route-matching-fields" className="text-xs font-semibold">{t('routes.form.matching')}</h3>
          <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_10rem]">
            <div className="grid gap-1.5">
              <Label htmlFor="route-pattern">{t('routes.form.modelPattern')}</Label>
              <Input id="route-pattern" className="font-mono text-xs" autoComplete="off" value={values.modelPattern} aria-invalid={Boolean(errors.modelPattern)} placeholder={t('routes.form.modelPatternPlaceholder')} onChange={(event) => updateValues({ modelPattern: event.target.value })} />
              <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('routes.form.modelPatternHint')}</p>
              {errors.modelPattern ? <p className="text-xs text-destructive">{t(`routes.validation.${errors.modelPattern}`)}</p> : null}
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="route-mode">{t('routes.form.mode')}</Label>
              <Select id="route-mode" value={values.mode} onChange={(event) => updateValues({ mode: event.target.value as RouteFormValues['mode'] })}>
                <option value="pattern">{t('routes.mode.pattern')}</option>
                <option value="explicit_group">{t('routes.mode.explicit_group')}</option>
              </Select>
            </div>
          </div>
          <div className="grid gap-1.5 sm:max-w-xs">
            <Label htmlFor="route-strategy">{t('routes.form.strategy')}</Label>
            <Select id="route-strategy" value={values.strategy} onChange={(event) => updateValues({ strategy: event.target.value as RouteFormValues['strategy'] })}>
              <option value="weighted">{t('routes.strategy.weighted')}</option>
              <option value="round_robin">{t('routes.strategy.round_robin')}</option>
              <option value="stable_first">{t('routes.strategy.stable_first')}</option>
            </Select>
            <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t(`routes.strategyHint.${values.strategy}`)}</p>
          </div>
        </section>

        <section className="grid gap-3" aria-labelledby="route-mapping-fields">
          <div><h3 id="route-mapping-fields" className="text-xs font-semibold">{t('routes.form.mapping')}</h3><p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('routes.form.mappingHint')}</p></div>
          <RouteMappingEditor rows={values.modelMapping} error={errors.mapping} onChange={(modelMapping) => updateValues({ modelMapping })} />
        </section>

        <section className="grid gap-3" aria-labelledby="route-candidates-fields">
          <div><h3 id="route-candidates-fields" className="text-xs font-semibold">{t('routes.form.candidates')}</h3><p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('routes.form.candidatesHint')}</p></div>
          <RouteCandidateEditor
            rows={values.candidates}
            channels={channels}
            catalogLoading={candidatesLoading}
            catalogError={candidatesError}
            onRetryCatalog={onRetryCandidates}
            formError={errors.candidates}
            onChange={(candidates) => updateValues({ candidates })}
          />
        </section>

        {submitError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t(`routes.errors.${routeWriteErrorCode(submitError)}`, { defaultValue: t('routes.errors.unknown') })}</p> : null}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="secondary" onClick={onCancel}>{t('routes.actions.cancel')}</Button>
        <Button type="submit" disabled={pending}>
          {pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
          {t(mode === 'create' ? 'routes.actions.create' : 'routes.actions.save')}
        </Button>
      </div>
    </form>
  )
}
