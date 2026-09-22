import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Chip, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Select, SelectItem, Textarea } from '@heroui/react'
import { ArrowRight, Check, Coins, Globe2, Image as ImageIcon, LoaderCircle } from 'lucide-react'
import { type ReactNode, useEffect, useState } from 'react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import type { AdminSiteSettings } from '@/lib/api/generated/types.gen'
import {
  balanceDisplayScaleChanged,
  formatQuota,
  formatRawQuota,
  type QuotaDisplayPolicy,
} from '@/lib/quota-display'
import { cn } from '@/lib/utils'
import { SiteBrandMark } from './site-brand-mark'
import { siteSettingsErrorCode, useUpdateAdminSiteSettings } from './site-settings-api'
import {
  balanceDisplayPolicy,
  buildSiteSettingsSchema,
  previewLogoUrl,
  siteSettingsBalanceDisplay,
  siteSettingsValues,
  toSiteSettingsRequest,
  type SiteSettingsValues,
} from './site-settings-form-model'

type SiteSettingsWorkspaceProps = {
  settings: AdminSiteSettings
}

/** 并列呈现结构化设置与公开品牌预览，预览只使用当前本地草稿。 */
export function SiteSettingsWorkspace({ settings }: SiteSettingsWorkspaceProps) {
  const { i18n, t } = useTranslation()
  const mutation = useUpdateAdminSiteSettings()
  const [pendingValues, setPendingValues] = useState<SiteSettingsValues>()
  const currentBalanceDisplay = siteSettingsBalanceDisplay(settings)
  const form = useForm<SiteSettingsValues>({
    defaultValues: siteSettingsValues(settings),
    resolver: zodResolver(buildSiteSettingsSchema({
      siteName: t('siteSettings.validation.siteName'),
      publicBaseUrl: t('siteSettings.validation.publicBaseUrl'),
      logoUrl: t('siteSettings.validation.logoUrl'),
      tagline: t('siteSettings.validation.tagline'),
      description: t('siteSettings.validation.description'),
      unitName: t('siteSettings.validation.unitName'),
      unitSymbol: t('siteSettings.validation.unitSymbol'),
      quotaUnitsPerDisplayUnit: t('siteSettings.validation.quotaUnitsPerDisplayUnit'),
      fractionDigits: t('siteSettings.validation.fractionDigits'),
    })),
  })

  useEffect(() => {
    form.reset(siteSettingsValues(settings))
  }, [form, settings])

  const save = async (values: SiteSettingsValues) => {
    try {
      const saved = await mutation.mutateAsync(toSiteSettingsRequest(values, settings.version))
      form.reset(siteSettingsValues(saved))
      setPendingValues(undefined)
    } catch {
      // 保留管理员草稿，响应错误不回显内部数据库诊断。
    }
  }

  const onSubmit = form.handleSubmit(async (values) => {
    if (balanceDisplayScaleChanged(
      currentBalanceDisplay,
      balanceDisplayPolicy(values),
    )) {
      setPendingValues(values)
      return
    }
    await save(values)
  })

  // HeroUI Select 不接受空字符串 key，动态选项走 items + 渲染函数。
  const symbolPositionItems = (['prefix', 'suffix'] as const).map((key) => ({
    key,
    label: t(`siteSettings.balanceDisplay.position.${key}`),
  }))
  const fractionDigitItems = [0, 1, 2, 3, 4].map((digits) => ({ key: String(digits), label: String(digits) }))
  const preview = form.watch()
  const previewName = preview.siteName.trim() || settings.site_name
  const previewTagline = preview.tagline.trim() || t('siteSettings.preview.taglineFallback')
  const previewDescription = preview.description.trim() || t('siteSettings.preview.descriptionFallback')
  const previewBalancePolicy = balanceDisplayPolicy(preview)
  const previewQuota = 20_000_000
  const saveError = siteSettingsErrorCode(mutation.error)
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const previewDisplayValue = safeFormatQuota(previewQuota, previewBalancePolicy, locale)
  const currentDisplayValue = safeFormatQuota(
    previewQuota,
    currentBalanceDisplay,
    locale,
  )

  return (
    <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,1.35fr)_minmax(17rem,0.65fr)]">
      <form
        className="min-w-0 overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1/55"
        onSubmit={onSubmit}
        noValidate
      >
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--hairline)] px-4 py-3.5">
          <div>
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="text-sm font-semibold">{t('siteSettings.form.title')}</h3>
              <Chip className="border-info/25 bg-info/10 text-info" size="sm" variant="flat">
                {t('siteSettings.form.version', { version: settings.version })}
              </Chip>
            </div>
            <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">
              {t('siteSettings.form.description')}
            </p>
          </div>
          <Globe2 className="size-4 text-info" aria-hidden="true" />
        </div>

        <section className="grid gap-4 border-b border-[var(--hairline)] p-4 sm:grid-cols-2" aria-labelledby="site-identity-title">
          <div className="sm:col-span-2">
            <h3 id="site-identity-title" className="text-sm font-semibold">{t('siteSettings.identity.title')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('siteSettings.identity.description')}</p>
          </div>
          <SettingsField
            id="site-settings-name"
            label={t('siteSettings.fields.siteName')}
            error={form.formState.errors.siteName?.message}
          >
            <Input
              id="site-settings-name"
              autoComplete="organization"
              isInvalid={!!form.formState.errors.siteName}
              size="sm"
              {...form.register('siteName')}
            />
          </SettingsField>
          <SettingsField
            id="site-settings-public-base"
            label={t('siteSettings.fields.publicBaseUrl')}
            hint={t('siteSettings.fields.publicBaseUrlHint')}
            error={form.formState.errors.publicBaseUrl?.message}
          >
            <Input
              id="site-settings-public-base"
              isInvalid={!!form.formState.errors.publicBaseUrl}
              placeholder="https://gateway.example.com"
              size="sm"
              type="url"
              {...form.register('publicBaseUrl')}
            />
          </SettingsField>
        </section>

        <section className="grid gap-4 border-b border-[var(--hairline)] p-4 sm:grid-cols-2" aria-labelledby="site-brand-title">
          <div className="sm:col-span-2">
            <h3 id="site-brand-title" className="text-sm font-semibold">{t('siteSettings.brand.title')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('siteSettings.brand.description')}</p>
          </div>
          <SettingsField
            id="site-settings-logo"
            label={t('siteSettings.fields.logoUrl')}
            hint={t('siteSettings.fields.logoUrlHint')}
            error={form.formState.errors.logoUrl?.message}
            className="sm:col-span-2"
          >
            <Input
              id="site-settings-logo"
              isInvalid={!!form.formState.errors.logoUrl}
              placeholder="/brand.svg"
              size="sm"
              {...form.register('logoUrl')}
            />
          </SettingsField>
          <SettingsField
            id="site-settings-tagline"
            label={t('siteSettings.fields.tagline')}
            error={form.formState.errors.tagline?.message}
            className="sm:col-span-2"
          >
            <Input
              id="site-settings-tagline"
              isInvalid={!!form.formState.errors.tagline}
              size="sm"
              {...form.register('tagline')}
            />
          </SettingsField>
          <SettingsField
            id="site-settings-description"
            label={t('siteSettings.fields.description')}
            error={form.formState.errors.description?.message}
            className="sm:col-span-2"
          >
            <Textarea
              id="site-settings-description"
              isInvalid={!!form.formState.errors.description}
              minRows={4}
              size="sm"
              {...form.register('description')}
            />
          </SettingsField>
        </section>

        <section className="grid gap-4 p-4" aria-labelledby="balance-display-title">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <div className="flex flex-wrap items-center gap-2">
                <h3 id="balance-display-title" className="text-sm font-semibold">{t('siteSettings.balanceDisplay.title')}</h3>
                <Chip className="bg-surface-2 text-muted-foreground" size="sm" variant="flat">{t('siteSettings.balanceDisplay.displayOnly')}</Chip>
              </div>
              <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('siteSettings.balanceDisplay.description')}</p>
            </div>
            <Coins className="size-4 text-info" aria-hidden="true" />
          </div>

          <div className="grid grid-cols-2 gap-1 rounded-lg bg-surface-2/65 p-1" role="group" aria-label={t('siteSettings.balanceDisplay.modeLabel')}>
            {(['quota', 'custom_unit'] as const).map((mode) => (
              <Button
                key={mode}
                type="button"
                size="sm"
                variant={preview.balanceMode === mode ? 'bordered' : 'light'}
                aria-pressed={preview.balanceMode === mode}
                onClick={() => form.setValue('balanceMode', mode, { shouldDirty: true, shouldValidate: true })}
              >
                {t(`siteSettings.balanceDisplay.mode.${mode}`)}
              </Button>
            ))}
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <SettingsField
              id="site-settings-unit-name"
              label={t('siteSettings.balanceDisplay.unitName')}
              hint={t('siteSettings.balanceDisplay.unitNameHint')}
              error={form.formState.errors.unitName?.message}
            >
              <Input
                id="site-settings-unit-name"
                isDisabled={preview.balanceMode !== 'custom_unit'}
                isInvalid={!!form.formState.errors.unitName}
                size="sm"
                {...form.register('unitName')}
              />
            </SettingsField>
            <SettingsField
              id="site-settings-unit-symbol"
              label={t('siteSettings.balanceDisplay.unitSymbol')}
              hint={t('siteSettings.balanceDisplay.unitSymbolHint')}
              error={form.formState.errors.unitSymbol?.message}
            >
              <Input
                id="site-settings-unit-symbol"
                isDisabled={preview.balanceMode !== 'custom_unit'}
                isInvalid={!!form.formState.errors.unitSymbol}
                size="sm"
                {...form.register('unitSymbol')}
              />
            </SettingsField>
            <SettingsField
              id="site-settings-quota-scale"
              label={t('siteSettings.balanceDisplay.quotaScale')}
              hint={t('siteSettings.balanceDisplay.quotaScaleHint')}
              error={form.formState.errors.quotaUnitsPerDisplayUnit?.message}
            >
              <Input
                id="site-settings-quota-scale"
                inputMode="numeric"
                isDisabled={preview.balanceMode !== 'custom_unit'}
                isInvalid={!!form.formState.errors.quotaUnitsPerDisplayUnit}
                size="sm"
                {...form.register('quotaUnitsPerDisplayUnit')}
              />
            </SettingsField>
            <SettingsField
              id="site-settings-symbol-position"
              label={t('siteSettings.balanceDisplay.symbolPosition')}
            >
              <Controller
                control={form.control}
                name="symbolPosition"
                render={({ field }) => (
                  <Select
                    aria-label={t('siteSettings.balanceDisplay.symbolPosition')}
                    id="site-settings-symbol-position"
                    isDisabled={preview.balanceMode !== 'custom_unit'}
                    items={symbolPositionItems}
                    selectedKeys={[field.value]}
                    size="sm"
                    onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? 'prefix'))}
                  >
                    {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                  </Select>
                )}
              />
            </SettingsField>
            <SettingsField
              id="site-settings-fraction-digits"
              label={t('siteSettings.balanceDisplay.fractionDigits')}
              hint={t('siteSettings.balanceDisplay.fractionDigitsHint')}
              error={form.formState.errors.fractionDigits?.message}
              className="sm:col-span-2"
            >
              <Controller
                control={form.control}
                name="fractionDigits"
                render={({ field }) => (
                  <Select
                    aria-label={t('siteSettings.balanceDisplay.fractionDigits')}
                    id="site-settings-fraction-digits"
                    isDisabled={preview.balanceMode !== 'custom_unit'}
                    items={fractionDigitItems}
                    selectedKeys={[String(field.value)]}
                    size="sm"
                    onSelectionChange={(keys) => field.onChange(Number(Array.from(keys)[0] ?? 0))}
                  >
                    {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                  </Select>
                )}
              />
            </SettingsField>
          </div>
        </section>

        <div className="flex min-h-14 flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] bg-surface-2/25 px-4 py-3">
          <div className="text-xs">
            {mutation.isError ? (
              <span role="alert" className="text-destructive">
                {t(saveError === 'site_settings_conflict' ? 'siteSettings.errors.conflict' : 'siteSettings.errors.save')}
              </span>
            ) : mutation.isSuccess && !form.formState.isDirty ? (
              <span className="inline-flex items-center gap-1.5 text-success">
                <Check className="size-3.5" aria-hidden="true" />
                {t('siteSettings.state.saved')}
              </span>
            ) : form.formState.isDirty ? (
              <span className="text-muted-foreground">{t('siteSettings.state.unsaved')}</span>
            ) : (
              <span className="text-muted-foreground">{t('siteSettings.state.synced')}</span>
            )}
          </div>
          <Button type="submit" color="primary" isDisabled={!form.formState.isDirty || mutation.isPending}>
            {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
            {t(mutation.isPending ? 'siteSettings.actions.saving' : 'siteSettings.actions.save')}
          </Button>
        </div>
      </form>

      <aside className="overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1/55">
        <div className="flex items-center justify-between border-b border-[var(--hairline)] px-4 py-3.5">
          <div>
            <h3 className="text-sm font-semibold">{t('siteSettings.preview.title')}</h3>
            <p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('siteSettings.preview.description')}</p>
          </div>
          <ImageIcon className="size-4 text-muted-foreground" aria-hidden="true" />
        </div>
        <div className="relative isolate min-h-72 overflow-hidden p-5">
          <div className="pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(circle_at_75%_20%,color-mix(in_oklab,var(--info)_18%,transparent),transparent_48%)]" />
          <div className="flex items-center gap-3">
            <SiteBrandMark
              siteName={previewName}
              logoUrl={previewLogoUrl(preview.logoUrl)}
            />
            <div className="min-w-0">
              <p className="truncate text-sm font-semibold">{previewName}</p>
              <p className="truncate text-[0.6875rem] text-muted-foreground">{t('brand.scope')}</p>
            </div>
          </div>
          <div className="mt-12">
            <p className="text-xs font-medium text-info">{previewTagline}</p>
            <h4 className="mt-3 max-w-[12ch] text-3xl leading-[1.08] font-semibold">{previewName}</h4>
            <p className="mt-4 max-w-[30ch] text-sm leading-6 text-muted-foreground">{previewDescription}</p>
          </div>
          {preview.publicBaseUrl.trim() ? (
            <p className="mt-8 truncate rounded-lg border border-[var(--hairline)] bg-background/65 px-3 py-2 font-mono text-[0.6875rem] text-muted-foreground">
              {preview.publicBaseUrl.trim()}
            </p>
          ) : null}
        </div>
        <div className="border-t border-[var(--hairline)] p-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <p className="text-xs font-semibold">{t('siteSettings.balanceDisplay.previewTitle')}</p>
              <p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('siteSettings.balanceDisplay.previewDescription')}</p>
            </div>
            <Coins className="size-4 text-info" aria-hidden="true" />
          </div>
          <div className="mt-3 grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-2 text-sm tabular-nums">
            <div className="min-w-0">
              <p className="truncate font-medium">{formatRawQuota(previewQuota, locale)}</p>
              <p className="mt-0.5 text-[0.6875rem] text-muted-foreground">quota</p>
            </div>
            <ArrowRight className="size-3.5 text-muted-foreground" aria-hidden="true" />
            <div className="min-w-0 text-right">
              <p className="truncate font-semibold text-info" title={previewDisplayValue}>{previewDisplayValue}</p>
              <p className="mt-0.5 truncate text-[0.6875rem] text-muted-foreground">
                {preview.balanceMode === 'quota' ? t('siteSettings.balanceDisplay.rawUnit') : preview.unitName}
              </p>
            </div>
          </div>
        </div>
      </aside>

      <Modal
        backdrop="blur"
        hideCloseButton
        isDismissable={false}
        isOpen={pendingValues !== undefined}
        onOpenChange={(open) => { if (!open && !mutation.isPending) setPendingValues(undefined) }}
      >
        <ModalContent>
          {() => (
            <>
              <ModalHeader className="grid gap-1.5">
                <h2 className="text-base font-semibold">{t('siteSettings.balanceDisplay.confirm.title')}</h2>
                <p className="text-sm leading-5 font-normal text-muted-foreground">{t('siteSettings.balanceDisplay.confirm.description')}</p>
              </ModalHeader>
              <ModalBody className="gap-3">
                <div className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-2 rounded-lg bg-surface-2/65 p-3 text-sm tabular-nums">
                  <span className="min-w-0 truncate" title={currentDisplayValue}>{currentDisplayValue}</span>
                  <ArrowRight className="size-3.5 text-muted-foreground" aria-hidden="true" />
                  <span className="min-w-0 truncate text-right font-semibold text-info" title={previewDisplayValue}>{previewDisplayValue}</span>
                </div>
                <p className="text-xs leading-5 font-normal text-muted-foreground">{t('siteSettings.balanceDisplay.confirm.safety')}</p>
              </ModalBody>
              <ModalFooter>
                <Button variant="light" isDisabled={mutation.isPending} onPress={() => setPendingValues(undefined)}>{t('siteSettings.balanceDisplay.confirm.cancel')}</Button>
                <Button color="primary" isDisabled={mutation.isPending || !pendingValues} onPress={() => { if (pendingValues) void save(pendingValues) }}>
                  {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
                  {t('siteSettings.balanceDisplay.confirm.save')}
                </Button>
              </ModalFooter>
            </>
          )}
        </ModalContent>
      </Modal>
    </div>
  )
}

function safeFormatQuota(value: number, policy: QuotaDisplayPolicy, locale: string) {
  try {
    return formatQuota(value, policy, locale)
  } catch {
    return '--'
  }
}

type SettingsFieldProps = {
  children: ReactNode
  className?: string
  error?: string
  hint?: string
  id: string
  label: string
}

function SettingsField({ children, className, error, hint, id, label }: SettingsFieldProps) {
  return (
    <div className={cn('grid content-start gap-1.5', className)}>
      <label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {!error && hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}
