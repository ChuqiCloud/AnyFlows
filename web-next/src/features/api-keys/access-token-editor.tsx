import { useState } from 'react'
import { Button, Checkbox, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Select, SelectItem, Switch, Textarea } from '@heroui/react'
import { LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { endManagementSession } from '@/features/auth/session-query'
import { navigateTo } from '@/lib/router-navigation'
import { resolveLoginPath } from '@/shared/auth/kernel'
import type { AccessToken, AccessTokenScope, AccessTokenWrite } from './access-token-api'
import { accessTokenDraftKey, accessTokenErrorKey, dateChoices, dateInputValue, errorCode, expiryTimestamp, ipEntry, needsElevation, type AccessTokenDraft } from './access-token-model'

export function AccessTokenEditor({ token, ownerId, isAdmin, catalog, groupedScopes, initialDraft, isOpen, pending, error, onClose, onSubmit }: {
  token?: AccessToken
  ownerId?: number
  isAdmin: boolean | undefined
  catalog?: { scopes: AccessTokenScope[]; presets: Array<{ id: string; label: string; scopes: string[] }>; default_expiry_days: number; max_expiry_days: number; allow_permanent: boolean }
  groupedScopes: Array<[string, AccessTokenScope[]]>
  initialDraft?: AccessTokenDraft
  isOpen: boolean
  pending: boolean
  error: unknown
  onClose: () => void
  onSubmit: (body: AccessTokenWrite, elevated: boolean) => Promise<void>
}) {
  const { t } = useTranslation()
  const draft = initialDraft && initialDraft.token_id === token?.id ? initialDraft : undefined
  const [name, setName] = useState(draft?.name ?? token?.name ?? '')
  const [scopes, setScopes] = useState<string[]>(draft?.scopes ?? token?.scopes ?? [])
  const [expiryChoice, setExpiryChoice] = useState<number | 'custom' | 'never' | 'unchanged'>(() => {
    if (draft) return draft.expiryChoice
    if (!token) return catalog?.default_expiry_days ?? 30
    return 'unchanged'
  })
  const [customDate, setCustomDate] = useState(draft?.customDate ?? dateInputValue(token?.expires_at ?? null))
  const [allowIpsText, setAllowIpsText] = useState(draft?.allowIpsText ?? token?.allow_ips?.join('\n') ?? '')
  const [enabled, setEnabled] = useState(draft?.enabled ?? token?.enabled ?? true)
  const [formError, setFormError] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [reauthRequired, setReauthRequired] = useState(false)
  const [selectedPreset, setSelectedPreset] = useState('custom')
  const [scopeSearch, setScopeSearch] = useState('')

  const parsedAllowIps = allowIpsText.split(/[\r\n,]+/u).map((value) => value.trim()).filter(Boolean)
  const expiry = expiryChoice === 'unchanged' ? token?.expires_at : expiryTimestamp(expiryChoice, customDate)
  const valid = name.trim().length > 0 && new TextEncoder().encode(name.trim()).length <= 128 && !/\p{Cc}/u.test(name)
    && scopes.length > 0 && expiry !== undefined
    && (expiry === null || Number.isSafeInteger(expiry))
    && (expiryChoice !== 'custom' || (Date.parse(customDate) > Date.now() && Date.parse(customDate) <= Date.now() + (catalog?.max_expiry_days ?? 365) * 86_400_000))
    && parsedAllowIps.length <= 64 && parsedAllowIps.every((value) => ipEntry.safeParse(value).success)
  const searchedGroups = groupedScopes.map(([group, entries]) => [group, entries.filter((scope) =>
    (!scope.admin_only || isAdmin)
    && (!scopeSearch.trim() || `${scope.scope} ${scope.label} ${scope.description}`.toLowerCase().includes(scopeSearch.trim().toLowerCase())),
  )] as const).filter(([, entries]) => entries.length > 0)
  const expiryOptions = [
    ...(token ? [{ id: 'unchanged', label: t('accessTokens.expiryUnchanged') }] : []),
    ...dateChoices.filter((days) => days <= (catalog?.max_expiry_days ?? 365)).map((days) => ({ id: String(days), label: t('accessTokens.expiryDays', { count: days }) })),
    { id: 'custom', label: t('accessTokens.expiryCustom') },
    ...(catalog?.allow_permanent ? [{ id: 'never', label: t('accessTokens.never') }] : []),
  ]

  const applyPreset = (preset: { id: string; scopes: string[] }) => {
    setScopes(preset.scopes)
    setSelectedPreset(preset.id)
  }
  const toggleScope = (scope: string, selected: boolean) => {
    setScopes((current) => selected ? [...new Set([...current, scope])] : current.filter((value) => value !== scope))
    setSelectedPreset('custom')
  }

  const submit = async () => {
    setFormError('')
    setReauthRequired(false)
    if (!valid || expiry === undefined) { setFormError(t('accessTokens.errors.invalidForm')); return }
    setSubmitting(true)
    try {
      const body = { name: name.trim(), scopes, expires_at: expiry, allow_ips: parsedAllowIps.length ? parsedAllowIps : null, enabled, version: token?.version }
      await onSubmit(body, token ? needsElevation(token, body) : true)
    } catch (error) {
      const code = errorCode(error)
      if (code === 'recent_authentication_required') {
        setReauthRequired(true)
        setFormError('')
      } else {
        setFormError(t(accessTokenErrorKey(error)))
      }
    } finally {
      setSubmitting(false)
    }
  }

  const reauthenticate = () => {
    try {
      if (!ownerId) return
      window.sessionStorage.setItem(accessTokenDraftKey, JSON.stringify({ owner_id: ownerId, token_id: token?.id, name, scopes, expiryChoice, customDate, allowIpsText, enabled }))
    } catch {
      setFormError(t('accessTokens.errors.draftStorage'))
      return
    }
    endManagementSession()
    navigateTo(resolveLoginPath('/console/api-keys?tab=system'))
  }

  const isPending = pending || submitting
  return (
    <Modal backdrop="blur" isDismissable={!isPending} isKeyboardDismissDisabled={isPending} isOpen={isOpen} onOpenChange={(open) => !open && !isPending && onClose()} scrollBehavior="inside" size="2xl">
      <ModalContent>
        {() => <>
          <ModalHeader className="flex flex-col gap-1"><h2 className="text-base font-semibold">{t(token ? 'accessTokens.editor.editTitle' : 'accessTokens.editor.createTitle')}</h2><p className="text-xs font-normal leading-5 text-muted-foreground">{t('accessTokens.editor.subtitle')}</p></ModalHeader>
          <ModalBody className="grid gap-4">
            <div className="grid gap-1.5"><label htmlFor="access-token-name" className="text-xs font-medium">{t('accessTokens.editor.name')}</label><Input id="access-token-name" autoFocus maxLength={128} size="sm" value={name} isDisabled={isPending} onValueChange={setName} /></div>
            <div className="grid gap-1.5"><label htmlFor="access-token-expiry" className="text-xs font-medium">{t('accessTokens.editor.expiry')}</label><Select id="access-token-expiry" aria-label={t('accessTokens.editor.expiry')} selectedKeys={[String(expiryChoice)]} disallowEmptySelection isDisabled={isPending} size="sm" onSelectionChange={(keys) => { const value = String([...keys][0]); setExpiryChoice(value === 'custom' || value === 'never' || value === 'unchanged' ? value : Number(value)) }}>
              {expiryOptions.map((option) => <SelectItem key={option.id}>{option.label}</SelectItem>)}
            </Select>{expiryChoice === 'custom' ? <Input aria-label={t('accessTokens.editor.customExpiry')} type="datetime-local" size="sm" min={dateInputValue(Math.floor(Date.now() / 1000) + 60)} value={customDate} onValueChange={setCustomDate} /> : null}</div>
            <div className="grid gap-2">
              <div className="flex flex-wrap items-center justify-between gap-2"><h3 className="text-xs font-semibold">{t('accessTokens.editor.permissions')}</h3><span className="text-[11px] text-muted-foreground">{t('accessTokens.editor.selectedCount', { count: scopes.length })}</span></div>
              <div className="flex flex-wrap gap-1.5">{(catalog?.presets ?? []).map((preset) => <Button key={preset.id} type="button" size="sm" isDisabled={isPending} variant={selectedPreset === preset.id ? 'flat' : 'light'} onPress={() => applyPreset(preset)}>{t(`accessTokens.presets.${preset.id}`, { defaultValue: preset.label })}</Button>)}<Button type="button" size="sm" isDisabled={isPending} variant={selectedPreset === 'custom' ? 'flat' : 'light'} onPress={() => setSelectedPreset('custom')}>{t('accessTokens.presets.custom')}</Button></div>
              <p className="text-[11px] leading-4 text-muted-foreground">{isAdmin ? t('accessTokens.editor.adminHint') : t('accessTokens.editor.userHint')}</p>
              <Input size="sm" type="search" aria-label={t('accessTokens.editor.searchPermissions')} placeholder={t('accessTokens.editor.searchPermissions')} value={scopeSearch} onValueChange={setScopeSearch} />
              <div className="grid max-h-80 gap-3 overflow-y-auto pr-1">{searchedGroups.map(([group, entries]) => <fieldset key={group} className="grid content-start gap-2 border-t border-[var(--hairline)] pt-2"><legend className="text-xs font-medium">{t(`accessTokens.scopeGroups.${group}`, { defaultValue: group })}</legend><div className="grid gap-2 sm:grid-cols-2">{entries.map((scope) => <Checkbox key={scope.scope} size="sm" isDisabled={isPending} classNames={{ base: 'm-0 max-w-full items-start p-0', label: 'grid min-w-0 gap-0.5' }} isSelected={scopes.includes(scope.scope)} onValueChange={(checked) => toggleScope(scope.scope, checked)}><span className="text-xs font-medium">{scope.label}</span><span className="text-[11px] leading-4 text-muted-foreground">{scope.description}</span></Checkbox>)}</div></fieldset>)}</div>
              {searchedGroups.length === 0 ? <p className="text-xs text-muted-foreground">{t('accessTokens.editor.noPermissions')}</p> : null}
            </div>
            <div className="grid gap-1.5"><label htmlFor="access-token-ips" className="text-xs font-medium">{t('accessTokens.editor.allowIps')}</label><Textarea id="access-token-ips" minRows={2} maxRows={6} size="sm" isDisabled={isPending} value={allowIpsText} onValueChange={setAllowIpsText} placeholder={t('accessTokens.editor.allowIpsHint')} /><p className="text-[11px] leading-4 text-muted-foreground">{t('accessTokens.editor.allowIpsDescription')}</p></div>
            {token ? <div className="flex items-center justify-between gap-3 border-t border-[var(--hairline)] pt-3"><div><p className="text-xs font-medium">{t('accessTokens.editor.enabled')}</p><p className="text-[11px] text-muted-foreground">{t('accessTokens.editor.enabledHint')}</p></div><Switch aria-label={t('accessTokens.editor.enabled')} size="sm" isDisabled={isPending} isSelected={enabled} onValueChange={setEnabled} /></div> : null}
            {reauthRequired ? <div className="grid gap-2 border-t border-[var(--hairline)] pt-3"><p role="alert" className="text-xs leading-5 text-warning">{t('accessTokens.errors.recentAuthentication')}</p><Button type="button" size="sm" variant="bordered" className="justify-self-start" onPress={reauthenticate}>{t('accessTokens.actions.reauthenticate')}</Button></div> : null}
            {formError ? <p role="alert" className="text-xs text-destructive">{formError}</p> : null}
            {error && !formError && !reauthRequired ? <p role="alert" className="text-xs text-destructive">{t(accessTokenErrorKey(error))}</p> : null}
          </ModalBody>
          <ModalFooter><Button type="button" variant="light" isDisabled={isPending} onPress={onClose}>{t('accessTokens.actions.cancel')}</Button><Button type="button" color="primary" isDisabled={!valid || isPending} onPress={() => void submit()}>{isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}{t(token ? 'accessTokens.actions.save' : 'accessTokens.actions.create')}</Button></ModalFooter>
        </>}
      </ModalContent>
    </Modal>
  )
}

