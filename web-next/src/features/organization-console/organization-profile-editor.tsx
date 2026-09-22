import { Button, Input } from '@heroui/react'
import { CheckCircle2, LoaderCircle, Save } from 'lucide-react'
import { useEffect, useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { useUpdateOrganizationProfile } from '@/features/organization-budgets/organization-budget-api'
import type { OrganizationOverviewResponse } from '@/lib/api/generated/types.gen'

const SLUG_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/

/** 企业初始化与设置共用同一份 CAS 资料编辑能力。 */
export function OrganizationProfileEditor({
  organizationId,
  overview,
}: {
  organizationId: number
  overview: OrganizationOverviewResponse
}) {
  const { t } = useTranslation()
  const mutation = useUpdateOrganizationProfile(organizationId)
  const [name, setName] = useState(overview.name)
  const [slug, setSlug] = useState(overview.slug)
  const [saved, setSaved] = useState(false)

  useEffect(() => {
    setName(overview.name)
    setSlug(overview.slug)
  }, [overview.name, overview.slug])

  const normalizedName = name.trim()
  const normalizedSlug = slug.trim().toLowerCase()
  const valid = normalizedName.length > 0
    && new TextEncoder().encode(normalizedName).length <= 128
    && normalizedSlug.length <= 64
    && SLUG_PATTERN.test(normalizedSlug)
  const changed = normalizedName !== overview.name || normalizedSlug !== overview.slug

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!valid || !changed) return
    setSaved(false)
    try {
      await mutation.mutateAsync({
        name: normalizedName,
        slug: normalizedSlug,
        expected_version: overview.version,
      })
      setSaved(true)
    } catch {
      // 保留用户输入，CAS 冲突或服务错误可在刷新事实后重试。
    }
  }

  return (
    <form className="grid gap-4" onSubmit={(event) => void submit(event)}>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="grid gap-1.5">
          <label className="text-xs font-medium leading-none text-foreground" htmlFor={`organization-profile-name-${organizationId}`}>{t('organizationConsole.settings.identity.name')}</label>
          <Input
            id={`organization-profile-name-${organizationId}`}
            size="sm"
            value={name}
            maxLength={128}
            isDisabled={mutation.isPending}
            onChange={(event) => { setName(event.target.value); setSaved(false); mutation.reset() }}
          />
        </div>
        <div className="grid gap-1.5">
          <label className="text-xs font-medium leading-none text-foreground" htmlFor={`organization-profile-slug-${organizationId}`}>{t('organizationConsole.settings.identity.slug')}</label>
          <Input
            id={`organization-profile-slug-${organizationId}`}
            size="sm"
            value={slug}
            maxLength={64}
            autoCapitalize="none"
            autoCorrect="false"
            spellCheck="false"
            isDisabled={mutation.isPending}
            classNames={{ input: 'font-mono' }}
            onChange={(event) => { setSlug(event.target.value); setSaved(false); mutation.reset() }}
          />
          <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('organizationConsole.settings.identity.slugHint')}</p>
        </div>
      </div>
      {!valid && (name !== overview.name || slug !== overview.slug) ? (
        <p role="alert" className="text-xs text-destructive">{t('organizationConsole.settings.identity.invalid')}</p>
      ) : null}
      {mutation.isError ? (
        <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t('organizationConsole.settings.identity.error')}</p>
      ) : null}
      <div className="flex flex-wrap items-center gap-2">
        <Button type="submit" color="primary" size="sm" isDisabled={!valid || !changed || mutation.isPending}>
          {mutation.isPending ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : <Save className="size-3.5" aria-hidden="true" />}
          {t('organizationConsole.settings.identity.save')}
        </Button>
        <span className="text-xs text-muted-foreground">#{overview.id} · {t(`organizationConsole.roles.${overview.role}`)}</span>
        {saved ? <span className="flex items-center gap-1 text-xs text-success"><CheckCircle2 className="size-3.5" aria-hidden="true" />{t('organizationConsole.settings.identity.saved')}</span> : null}
      </div>
    </form>
  )
}
