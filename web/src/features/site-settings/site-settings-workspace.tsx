import { zodResolver } from '@hookform/resolvers/zod'
import { Check, Globe2, Image as ImageIcon, LoaderCircle } from 'lucide-react'
import { type ReactNode, useEffect } from 'react'
import { useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import type { AdminSiteSettings } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { SiteBrandMark } from './site-brand-mark'
import { useUpdateAdminSiteSettings } from './site-settings-api'
import {
  buildSiteSettingsSchema,
  previewLogoUrl,
  siteSettingsValues,
  toSiteSettingsRequest,
  type SiteSettingsValues,
} from './site-settings-form-model'

type SiteSettingsWorkspaceProps = {
  settings: AdminSiteSettings
}

/** 并列呈现结构化设置与公开品牌预览，预览只使用当前本地草稿。 */
export function SiteSettingsWorkspace({ settings }: SiteSettingsWorkspaceProps) {
  const { t } = useTranslation()
  const mutation = useUpdateAdminSiteSettings()
  const form = useForm<SiteSettingsValues>({
    defaultValues: siteSettingsValues(settings),
    resolver: zodResolver(buildSiteSettingsSchema({
      siteName: t('siteSettings.validation.siteName'),
      publicBaseUrl: t('siteSettings.validation.publicBaseUrl'),
      logoUrl: t('siteSettings.validation.logoUrl'),
      tagline: t('siteSettings.validation.tagline'),
      description: t('siteSettings.validation.description'),
    })),
  })

  useEffect(() => {
    form.reset(siteSettingsValues(settings))
  }, [form, settings])

  const onSubmit = form.handleSubmit(async (values) => {
    try {
      const saved = await mutation.mutateAsync(toSiteSettingsRequest(values))
      form.reset(siteSettingsValues(saved))
    } catch {
      // 保留管理员草稿，响应错误不回显内部数据库诊断。
    }
  })

  const preview = form.watch()
  const previewName = preview.siteName.trim() || settings.site_name
  const previewTagline = preview.tagline.trim() || t('siteSettings.preview.taglineFallback')
  const previewDescription = preview.description.trim() || t('siteSettings.preview.descriptionFallback')

  return (
    <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,1.35fr)_minmax(17rem,0.65fr)]">
      <form
        className="min-w-0 overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1/55 shadow-[var(--shadow-subtle)]"
        onSubmit={onSubmit}
        noValidate
      >
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--hairline)] px-4 py-3.5">
          <div>
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="text-sm font-semibold">{t('siteSettings.form.title')}</h3>
              <Badge className="border-info/25 bg-info/10 text-info">
                {t('siteSettings.form.version', { version: settings.version })}
              </Badge>
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
              aria-invalid={!!form.formState.errors.siteName}
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
              type="url"
              placeholder="https://gateway.example.com"
              aria-invalid={!!form.formState.errors.publicBaseUrl}
              {...form.register('publicBaseUrl')}
            />
          </SettingsField>
        </section>

        <section className="grid gap-4 p-4 sm:grid-cols-2" aria-labelledby="site-brand-title">
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
              placeholder="/brand.svg"
              aria-invalid={!!form.formState.errors.logoUrl}
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
              aria-invalid={!!form.formState.errors.tagline}
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
              rows={4}
              aria-invalid={!!form.formState.errors.description}
              {...form.register('description')}
            />
          </SettingsField>
        </section>

        <div className="flex min-h-14 flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] bg-surface-2/25 px-4 py-3">
          <div className="text-xs">
            {mutation.isError ? (
              <span role="alert" className="text-destructive">{t('siteSettings.errors.save')}</span>
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
          <Button type="submit" disabled={!form.formState.isDirty || mutation.isPending}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
            {t(mutation.isPending ? 'siteSettings.actions.saving' : 'siteSettings.actions.save')}
          </Button>
        </div>
      </form>

      <aside className="overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1/55 shadow-[var(--shadow-subtle)]">
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
      </aside>
    </div>
  )
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
      <Label htmlFor={id}>{label}</Label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {!error && hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}
