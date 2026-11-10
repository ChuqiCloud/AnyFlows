import { useState } from 'react'
import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { capabilityError, useDeleteUsageProfile, useWriteUsageProfile, type UsageProfile } from './key-capability-api'
import { CapabilityError, CapabilityField, CapabilityInput, capabilityInputClass, nullableDecimal, validCurrency, validDecimal } from './key-capability-fields'

export function UpstreamUsageProfileEditor({ profile, onClose, onSaved }: { profile?: UsageProfile; onClose: () => void; onSaved: (id: number) => void }) {
  const { t } = useTranslation()
  const mutation = useWriteUsageProfile()
  const deletion = useDeleteUsageProfile()
  const [name, setName] = useState(profile?.name ?? '')
  const [provider, setProvider] = useState<UsageProfile['provider']>(profile?.provider ?? 'new_api')
  const [baseUrl, setBaseUrl] = useState(profile?.base_url ?? '')
  const [userId, setUserId] = useState(profile?.platform_user_id ?? '')
  const [quotaPerUnit, setQuotaPerUnit] = useState(profile?.quota_per_unit ?? '')
  const [currency, setCurrency] = useState(profile?.currency ?? '')
  const [secret, setSecret] = useState('')
  const [confirmDelete, setConfirmDelete] = useState(false)
  const pending = mutation.isPending || deletion.isPending
  let validUrl = false
  try { const url = new URL(baseUrl); validUrl = url.protocol === 'https:' && !url.username && !url.password && !url.search && !url.hash } catch { /* Invalid addresses remain editable. */ }
  const valid = name.trim().length > 0 && new TextEncoder().encode(name.trim()).length <= 128 && validUrl && baseUrl.length <= 2048 && userId.trim().length <= 64 && secret.length <= 8192 && (profile !== undefined || secret.trim().length > 0) && validDecimal(quotaPerUnit, true) && validCurrency(currency)
  const save = async () => {
    if (!valid || pending) return
    try {
      const result = await mutation.mutateAsync({ id: profile?.id, body: { name: name.trim(), provider, base_url: baseUrl.trim(), platform_user_id: userId.trim() || null, quota_per_unit: nullableDecimal(quotaPerUnit), currency: currency.trim() || null, secret: secret.trim() || null, version: profile?.version ?? 0 } })
      setSecret(''); onSaved(result.id); onClose()
    } catch { /* Write-only secret stays local until retry or closing the editor. */ }
  }
  const remove = async () => { if (!profile || pending) return; try { await deletion.mutateAsync(profile.id); onClose() } catch { /* Referenced profiles remain available for editing. */ } }
  return <Modal isOpen size="2xl" scrollBehavior="inside" isDismissable={!pending} onOpenChange={(open) => { if (!open && !pending) onClose() }}><ModalContent className="min-w-0 bg-surface-1 text-foreground"><ModalHeader>{t(profile ? 'keyCapabilities.editProfile' : 'keyCapabilities.createProfile')}</ModalHeader><ModalBody className="gap-3">
    <p className="text-xs leading-5 text-muted-foreground">{t('keyCapabilities.profileHint')}</p>
    <div className="grid gap-3 sm:grid-cols-2"><CapabilityField label={t('keyCapabilities.profileName')}><CapabilityInput aria-label={t('keyCapabilities.profileName')} value={name} maxLength={128} disabled={pending} onChange={(event) => setName(event.target.value)} /></CapabilityField><CapabilityField label={t('keyCapabilities.provider')}><select aria-label={t('keyCapabilities.provider')} className={capabilityInputClass} disabled={pending} value={provider} onChange={(event) => setProvider(event.target.value as UsageProfile['provider'])}>{(['new_api', 'one_api', 'sub2api'] as const).map((value) => <option key={value} value={value}>{t(`keyCapabilities.providers.${value}`)}</option>)}</select></CapabilityField></div>
    <CapabilityField label={t('keyCapabilities.baseUrl')} hint={t('keyCapabilities.httpsHint')}><CapabilityInput aria-label={t('keyCapabilities.baseUrl')} type="url" maxLength={2048} value={baseUrl} disabled={pending} onChange={(event) => setBaseUrl(event.target.value)} /></CapabilityField>
    <CapabilityField label={t('keyCapabilities.platformUserId')} hint={t('keyCapabilities.optional')}><CapabilityInput aria-label={t('keyCapabilities.platformUserId')} value={userId} maxLength={64} disabled={pending} onChange={(event) => setUserId(event.target.value)} /></CapabilityField>
    <CapabilityField label={t('keyCapabilities.managementSecret')} hint={t(profile?.has_secret ? 'keyCapabilities.secretPreserved' : 'keyCapabilities.secretRequired')}><CapabilityInput aria-label={t('keyCapabilities.managementSecret')} type="password" autoComplete="new-password" value={secret} maxLength={8192} disabled={pending} onChange={(event) => setSecret(event.target.value)} /></CapabilityField>
    <div className="grid gap-3 sm:grid-cols-2"><CapabilityField label={t('keyCapabilities.quotaPerUnit')} hint={t('keyCapabilities.conversionHint')}><CapabilityInput aria-label={t('keyCapabilities.quotaPerUnit')} inputMode="decimal" value={quotaPerUnit} disabled={pending} onChange={(event) => setQuotaPerUnit(event.target.value)} /></CapabilityField><CapabilityField label={t('keyCapabilities.currency')}><CapabilityInput aria-label={t('keyCapabilities.currency')} value={currency} maxLength={16} disabled={pending} onChange={(event) => setCurrency(event.target.value)} /></CapabilityField></div>
    {mutation.isError ? <CapabilityError code={capabilityError(mutation.error)} /> : null}
    {!valid && (name || baseUrl || userId || secret || quotaPerUnit || currency) ? <CapabilityError code="profileInvalid" /> : null}
    {deletion.isError ? <CapabilityError code={capabilityError(deletion.error) === 'conflict' ? 'profileInUse' : capabilityError(deletion.error)} /> : null}
    {confirmDelete ? <p className="text-xs text-warning">{t('keyCapabilities.deleteProfileHint')}</p> : null}
  </ModalBody><ModalFooter className="flex-wrap">{profile ? <Button type="button" size="sm" color="danger" variant="light" isDisabled={pending} onPress={() => confirmDelete ? void remove() : setConfirmDelete(true)}>{t(confirmDelete ? 'keyCapabilities.confirmDeleteProfile' : 'keyCapabilities.deleteProfile')}</Button> : null}<Button type="button" size="sm" variant="light" isDisabled={pending} onPress={onClose}>{t('keyCapabilities.close')}</Button><Button type="button" size="sm" color="primary" isLoading={mutation.isPending} isDisabled={!valid || pending} onPress={() => void save()}>{t('keyCapabilities.saveProfile')}</Button></ModalFooter></ModalContent></Modal>
}
