import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Checkbox, Input, Select, SelectItem, Switch, Textarea } from '@heroui/react'
import { Controller, useFieldArray, useForm } from 'react-hook-form'
import type { UseFormRegisterReturn } from 'react-hook-form'
import { LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminGroup } from '@/lib/api/generated/types.gen'
import {
  groupWriteErrorCode,
  useCreateAdminGroup,
  useUpdateAdminGroup,
} from './group-api'
import { GroupFormField } from './group-form-field'
import { MAX_PEAK_RULES, newPeakRule, utcOptions } from './group-peak-model'
import {
  buildGroupFormSchema,
  defaultGroupFormValues,
  toGroupRequest,
  type GroupEditorMode,
  type GroupFormValues,
} from './group-form-model'

/** HeroUI Select 不接受空字符串 key，用哨兵 key 表达"不设置回退分组"。 */
const NO_FALLBACK_KEY = '__none__'

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
    invalidWeekdays: t('groups.validation.weekdays'),
    invalidModels: t('groups.validation.models'),
    overlappingRules: t('groups.validation.overlap'),
    invalidRules: t('groups.validation.rules'),
    invalidTimezone: t('groups.validation.timezone'),
  })
  const form = useForm<GroupFormValues>({
    defaultValues: defaultGroupFormValues(group),
    resolver: zodResolver(schema),
  })
  const peakEnabled = form.watch('peakEnabled')
  const peakRules = form.watch('peakRules')
  const peakFields = useFieldArray({ control: form.control, name: 'peakRules' })
  const fallbackOptions = groups.filter((item) => item.id !== group?.id)
  // HeroUI Select 的动态选项必须走 items + 渲染函数（数组子节点不被类型接受）。
  const fallbackItems = [
    { key: NO_FALLBACK_KEY, label: t('groups.values.noFallback') },
    ...fallbackOptions.map((item) => ({ key: String(item.id), label: `${item.display_name} (${item.name})` })),
  ]

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
      <div className="flex-1 space-y-6 overflow-y-auto px-4 pt-4 pb-5">
        <section className="grid gap-3" aria-labelledby="group-identity-fields">
          <h3 id="group-identity-fields" className="text-xs font-semibold">{t('groups.form.identity')}</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <GroupFormField id="group-name" label={t('groups.fields.name')} error={form.formState.errors.name?.message}>
              <Input autoComplete="off" id="group-name" isInvalid={!!form.formState.errors.name} size="sm" {...form.register('name')} />
            </GroupFormField>
            <GroupFormField id="group-display-name" label={t('groups.fields.displayName')} error={form.formState.errors.displayName?.message}>
              <Input autoComplete="off" id="group-display-name" isInvalid={!!form.formState.errors.displayName} size="sm" {...form.register('displayName')} />
            </GroupFormField>
          </div>
        </section>

        <section className="grid gap-3" aria-labelledby="group-pricing-fields">
          <h3 id="group-pricing-fields" className="text-xs font-semibold">{t('groups.form.pricing')}</h3>
          <GroupFormField id="group-ratio" label={t('groups.fields.ratio')} hint={t('groups.form.ratioHint')} error={form.formState.errors.ratio?.message}>
            <Input id="group-ratio" inputMode="decimal" isInvalid={!!form.formState.errors.ratio} size="sm" {...form.register('ratio')} />
          </GroupFormField>
          <Controller
            control={form.control}
            name="peakEnabled"
            render={({ field }) => (
              <ToggleRow
                checked={field.value}
                label={t('groups.fields.peakEnabled')}
                description={t('groups.form.peakDescription')}
                onCheckedChange={(checked) => {
                  field.onChange(checked)
                  if (checked && peakFields.fields.length === 0) peakFields.append(newPeakRule())
                }}
              />
            )}
          />
          {peakEnabled ? (
            <div className="grid gap-3 rounded-lg border border-[var(--hairline)] bg-surface-2/35 p-3">
              {peakFields.fields.map((field, index) => {
                const rule = peakRules[index]
                return <div key={field.id} className="grid gap-3 border-b border-[var(--hairline)] pb-3 last:border-b-0 last:pb-0">
                  <div className="grid gap-2 sm:grid-cols-2">
                    <GroupFormField id={`group-peak-ratio-${index}`} label={t('groups.fields.peakRatio')} error={form.formState.errors.peakRules?.[index]?.ratio?.message}>
                      <Input id={`group-peak-ratio-${index}`} inputMode="decimal" isInvalid={!!form.formState.errors.peakRules?.[index]?.ratio} size="sm" {...form.register(`peakRules.${index}.ratio`)} />
                    </GroupFormField>
                    <GroupFormField id={`group-peak-zone-${index}`} label={t('groups.fields.timezone')} hint={t('groups.form.timezoneHint')} error={form.formState.errors.peakRules?.[index]?.utcOffset?.message}>
                      <Select aria-label={t('groups.fields.timezone')} items={utcOptions(rule?.utcOffset ?? '480')} selectedKeys={[rule?.utcOffset ?? '480']} size="sm" disallowEmptySelection onSelectionChange={(keys) => form.setValue(`peakRules.${index}.utcOffset`, String(Array.from(keys)[0] ?? '480'), { shouldValidate: true })}>
                        {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                      </Select>
                    </GroupFormField>
                    <GroupFormField id={`group-peak-start-${index}`} label={t('groups.fields.peakStart')} error={form.formState.errors.peakRules?.[index]?.start?.message}>
                      <Input id={`group-peak-start-${index}`} isInvalid={!!form.formState.errors.peakRules?.[index]?.start} size="sm" step={1} type="time" {...form.register(`peakRules.${index}.start`)} />
                    </GroupFormField>
                    <GroupFormField id={`group-peak-end-${index}`} label={t('groups.fields.peakEnd')} hint={t('groups.form.utcHint')} error={form.formState.errors.peakRules?.[index]?.end?.message}>
                      <Input id={`group-peak-end-${index}`} isInvalid={!!form.formState.errors.peakRules?.[index]?.end} size="sm" placeholder="20:00 / 24:00" {...form.register(`peakRules.${index}.end`)} />
                    </GroupFormField>
                  </div>
                  <fieldset className="grid gap-2 text-xs">
                    <legend className="font-medium">{t('groups.fields.weekdays')}</legend>
                    <div className="flex flex-wrap gap-3">
                      {[1, 2, 3, 4, 5, 6, 7].map((day) => <Checkbox key={day} size="sm" isSelected={rule?.weekdays.includes(day) ?? false} onValueChange={(checked) => form.setValue(`peakRules.${index}.weekdays`, checked ? [...(rule?.weekdays ?? []), day].sort() : (rule?.weekdays ?? []).filter((value) => value !== day), { shouldValidate: true })}>{t(`groups.weekdays.${day}`)}</Checkbox>)}
                    </div>
                    {form.formState.errors.peakRules?.[index]?.weekdays?.message ? <p role="alert" className="text-destructive">{form.formState.errors.peakRules?.[index]?.weekdays?.message}</p> : null}
                  </fieldset>
                  <div className="grid gap-2">
                    <Select aria-label={t('groups.fields.modelScope')} items={[{ key: 'all', label: t('groups.values.allModels') }, { key: 'selected', label: t('groups.values.selectedModels') }]} selectedKeys={[rule?.modelScope ?? 'all']} size="sm" disallowEmptySelection onSelectionChange={(keys) => form.setValue(`peakRules.${index}.modelScope`, Array.from(keys)[0] === 'selected' ? 'selected' : 'all', { shouldValidate: true })}>{(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}</Select>
                    {rule?.modelScope === 'selected' ? <GroupFormField id={`group-peak-models-${index}`} label={t('groups.fields.models')} hint={t('groups.form.modelsHint')} error={form.formState.errors.peakRules?.[index]?.models?.message}>
                      <Textarea id={`group-peak-models-${index}`} isInvalid={!!form.formState.errors.peakRules?.[index]?.models} size="sm" {...form.register(`peakRules.${index}.models`)} />
                    </GroupFormField> : null}
                    <Button type="button" variant="light" onPress={() => peakFields.remove(index)}>{t('groups.actions.removeRule')}</Button>
                  </div>
                </div>
              })}
              <Button type="button" variant="bordered" isDisabled={peakFields.fields.length >= MAX_PEAK_RULES} onPress={() => peakFields.append(newPeakRule())}>{t('groups.actions.addRule')}</Button>
              {form.formState.errors.peakRules?.root?.message ? <p role="alert" className="text-xs text-destructive">{form.formState.errors.peakRules.root.message}</p> : null}
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
            <Controller
              control={form.control}
              name="fallbackGroupId"
              render={({ field }) => (
                <Select
                  aria-label={t('groups.fields.fallback')}
                  isInvalid={!!form.formState.errors.fallbackGroupId}
                  items={fallbackItems}
                  selectedKeys={[field.value === '' ? NO_FALLBACK_KEY : field.value]}
                  size="sm"
                  onSelectionChange={(keys) => {
                    const next = String(Array.from(keys)[0] ?? NO_FALLBACK_KEY)
                    field.onChange(next === NO_FALLBACK_KEY ? '' : next)
                  }}
                >
                  {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                </Select>
              )}
            />
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
        <Button type="button" variant="bordered" onClick={onCancel}>{t('groups.actions.cancel')}</Button>
        <Button color="primary" isDisabled={pending} type="submit">
          {pending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
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
  return <Input id={id} inputMode="numeric" isInvalid={invalid} min={0} size="sm" step={1} type="number" {...registration} />
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
      <Switch aria-label={label} isSelected={checked} size="sm" onValueChange={onCheckedChange} />
    </div>
  )
}
