import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { Controller, useForm } from 'react-hook-form'
import type { UseFormRegisterReturn } from 'react-hook-form'
import { LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { AdminGroup } from '@/lib/api/generated/types.gen'
import {
  groupWriteErrorCode,
  useCreateAdminGroup,
  useUpdateAdminGroup,
} from './group-api'
import { GroupFormField } from './group-form-field'
import {
  buildGroupFormSchema,
  defaultGroupFormValues,
  toGroupRequest,
  type GroupEditorMode,
  type GroupFormValues,
} from './group-form-model'

type GroupFormProps = {
  mode: GroupEditorMode
  group?: AdminGroup
  groups: AdminGroup[]
  onCancel: () => void
  onSaved: (group: AdminGroup) => void
}

export function GroupForm({ mode, group, groups, onCancel, onSaved }: GroupFormProps) {
  const { t } = useTranslation()
  const createMutation = useCreateAdminGroup()
  const updateMutation = useUpdateAdminGroup()
  const schema = buildGroupFormSchema(group?.id, {
    invalidName: t('groups.validation.name'),
    invalidDisplayName: t('groups.validation.displayName'),
    invalidRatio: t('groups.validation.ratio'),
    invalidNumber: t('groups.validation.number'),
    invalidGroup: t('groups.validation.fallback'),
    invalidPeakWindow: t('groups.validation.peakWindow'),
  })
  const form = useForm<GroupFormValues>({
    defaultValues: defaultGroupFormValues(group),
    resolver: zodResolver(schema),
  })
  const peakEnabled = form.watch('peakEnabled')
  const fallbackOptions = groups.filter((item) => item.id !== group?.id)

  useEffect(() => {
    form.reset(defaultGroupFormValues(group))
  }, [form, group, mode])

  const pending = createMutation.isPending || updateMutation.isPending
  const submitError = createMutation.error ?? updateMutation.error
  const submit = form.handleSubmit(async (values) => {
    try {
      const body = toGroupRequest(values, group)
      const saved = mode === 'create'
        ? await createMutation.mutateAsync(body)
        : group
          ? await updateMutation.mutateAsync({ id: group.id, body })
          : undefined
      if (saved) onSaved(saved)
    } catch {
      // 服务端失败时保留全部结构化字段，便于管理员修正或重试刷新。
    }
  })

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit} noValidate>
      <div className="flex-1 space-y-6 overflow-y-auto px-4 py-5">
        <section className="grid gap-3" aria-labelledby="group-identity-fields">
          <h3 id="group-identity-fields" className="text-xs font-semibold">{t('groups.form.identity')}</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <GroupFormField id="group-name" label={t('groups.fields.name')} error={form.formState.errors.name?.message}>
              <Input id="group-name" autoComplete="off" aria-invalid={!!form.formState.errors.name} {...form.register('name')} />
            </GroupFormField>
            <GroupFormField id="group-display-name" label={t('groups.fields.displayName')} error={form.formState.errors.displayName?.message}>
              <Input id="group-display-name" autoComplete="off" aria-invalid={!!form.formState.errors.displayName} {...form.register('displayName')} />
            </GroupFormField>
          </div>
        </section>

        <section className="grid gap-3" aria-labelledby="group-pricing-fields">
          <h3 id="group-pricing-fields" className="text-xs font-semibold">{t('groups.form.pricing')}</h3>
          <GroupFormField id="group-ratio" label={t('groups.fields.ratio')} hint={t('groups.form.ratioHint')} error={form.formState.errors.ratio?.message}>
            <Input id="group-ratio" inputMode="decimal" aria-invalid={!!form.formState.errors.ratio} {...form.register('ratio')} />
          </GroupFormField>
          <Controller
            control={form.control}
            name="peakEnabled"
            render={({ field }) => (
              <ToggleRow
                checked={field.value}
                label={t('groups.fields.peakEnabled')}
                description={t('groups.form.peakDescription')}
                onCheckedChange={field.onChange}
              />
            )}
          />
          {peakEnabled ? (
            <div className="grid gap-3 rounded-lg border border-[var(--hairline)] bg-surface-2/35 p-3 sm:grid-cols-3">
              <GroupFormField id="group-peak-ratio" label={t('groups.fields.peakRatio')} error={form.formState.errors.peakRatio?.message}>
                <Input id="group-peak-ratio" inputMode="decimal" aria-invalid={!!form.formState.errors.peakRatio} {...form.register('peakRatio')} />
              </GroupFormField>
              <GroupFormField id="group-peak-start" label={t('groups.fields.peakStart')} error={form.formState.errors.peakStart?.message}>
                <Input id="group-peak-start" type="time" step={1} aria-invalid={!!form.formState.errors.peakStart} {...form.register('peakStart')} />
              </GroupFormField>
              <GroupFormField id="group-peak-end" label={t('groups.fields.peakEnd')} hint={t('groups.form.utcHint')} error={form.formState.errors.peakEnd?.message}>
                <Input id="group-peak-end" type="time" step={1} aria-invalid={!!form.formState.errors.peakEnd} {...form.register('peakEnd')} />
              </GroupFormField>
            </div>
          ) : null}
        </section>

        <section className="grid gap-3" aria-labelledby="group-limit-fields">
          <h3 id="group-limit-fields" className="text-xs font-semibold">{t('groups.form.limits')}</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <GroupFormField id="group-daily-limit" label={t('groups.fields.dailyLimit')} error={form.formState.errors.dailyLimit?.message}>
              <LimitInput id="group-daily-limit" invalid={!!form.formState.errors.dailyLimit} registration={form.register('dailyLimit')} />
            </GroupFormField>
            <GroupFormField id="group-weekly-limit" label={t('groups.fields.weeklyLimit')} error={form.formState.errors.weeklyLimit?.message}>
              <LimitInput id="group-weekly-limit" invalid={!!form.formState.errors.weeklyLimit} registration={form.register('weeklyLimit')} />
            </GroupFormField>
            <GroupFormField id="group-monthly-limit" label={t('groups.fields.monthlyLimit')} error={form.formState.errors.monthlyLimit?.message}>
              <LimitInput id="group-monthly-limit" invalid={!!form.formState.errors.monthlyLimit} registration={form.register('monthlyLimit')} />
            </GroupFormField>
            <GroupFormField id="group-rpm-limit" label={t('groups.fields.rpmLimit')} hint={t('groups.form.optionalLimitHint')} error={form.formState.errors.rpmLimit?.message}>
              <LimitInput id="group-rpm-limit" invalid={!!form.formState.errors.rpmLimit} registration={form.register('rpmLimit')} />
            </GroupFormField>
          </div>
        </section>

        <section className="grid gap-3" aria-labelledby="group-routing-fields">
          <h3 id="group-routing-fields" className="text-xs font-semibold">{t('groups.form.routing')}</h3>
          <GroupFormField id="group-fallback" label={t('groups.fields.fallback')} hint={t('groups.form.fallbackHint')} error={form.formState.errors.fallbackGroupId?.message}>
            <Select id="group-fallback" aria-invalid={!!form.formState.errors.fallbackGroupId} {...form.register('fallbackGroupId')}>
              <option value="">{t('groups.values.noFallback')}</option>
              {fallbackOptions.map((item) => <option key={item.id} value={item.id}>{item.display_name} ({item.name})</option>)}
            </Select>
          </GroupFormField>
          <div className="grid gap-2">
            <Controller
              control={form.control}
              name="isExclusive"
              render={({ field }) => (
                <ToggleRow checked={field.value} label={t('groups.fields.exclusive')} description={t('groups.form.exclusiveDescription')} onCheckedChange={field.onChange} />
              )}
            />
            <Controller
              control={form.control}
              name="claudeCodeOnly"
              render={({ field }) => (
                <ToggleRow checked={field.value} label={t('groups.fields.claudeCodeOnly')} description={t('groups.form.claudeCodeDescription')} onCheckedChange={field.onChange} />
              )}
            />
          </div>
        </section>

        {submitError ? (
          <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {t(`groups.errors.${groupWriteErrorCode(submitError) ?? 'unknown'}`, { defaultValue: t('groups.errors.unknown') })}
          </p>
        ) : null}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="secondary" onClick={onCancel}>{t('groups.actions.cancel')}</Button>
        <Button type="submit" disabled={pending}>
          {pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
          {t(mode === 'create' ? 'groups.actions.create' : 'groups.actions.save')}
        </Button>
      </div>
    </form>
  )
}

type LimitInputProps = {
  id: string
  invalid: boolean
  registration: UseFormRegisterReturn
}

function LimitInput({ id, invalid, registration }: LimitInputProps) {
  return <Input id={id} type="number" min={0} step={1} inputMode="numeric" aria-invalid={invalid} {...registration} />
}

function ToggleRow({
  checked,
  label,
  description,
  onCheckedChange,
}: {
  checked: boolean
  label: string
  description: string
  onCheckedChange: (checked: boolean) => void
}) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
      <div className="min-w-0">
        <p className="text-xs font-medium">{label}</p>
        <p className="mt-0.5 text-[0.6875rem] leading-4 text-muted-foreground">{description}</p>
      </div>
      <Switch checked={checked} aria-label={label} onCheckedChange={onCheckedChange} />
    </div>
  )
}
