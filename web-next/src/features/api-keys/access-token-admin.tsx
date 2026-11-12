import { useState } from 'react'
import { Button, Input, Modal, ModalContent, ModalFooter, ModalHeader, Skeleton, Switch } from '@heroui/react'
import { Trash2, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'

export function AdminTokenControls({ policy, loading, onChange, userId, onUserIdChange, onRevokeUser, pending, error, revokedCount }: {
  policy?: { enabled: boolean; issuance_enabled: boolean; max_tokens_per_user: number; version: number }
  loading: boolean
  onChange: (patch: Partial<{ enabled: boolean; issuance_enabled: boolean; max_tokens_per_user: number }>) => void
  userId: string
  onUserIdChange: (value: string) => void
  onRevokeUser: () => void
  pending: boolean
  error: boolean
  revokedCount?: number
}) {
  const { t } = useTranslation()
  const [confirmStop, setConfirmStop] = useState(false)
  const [confirmRevokeUser, setConfirmRevokeUser] = useState(false)
  const [enabled, setEnabled] = useState(policy?.enabled ?? true)
  const [issuanceEnabled, setIssuanceEnabled] = useState(policy?.issuance_enabled ?? true)
  const [capacity, setCapacity] = useState(String(policy?.max_tokens_per_user ?? 20))
  const changed = Boolean(policy && (policy.enabled !== enabled || policy.issuance_enabled !== issuanceEnabled || String(policy.max_tokens_per_user) !== capacity))
  const save = () => {
    if (!/^[1-9]\d*$/.test(capacity) || !Number.isSafeInteger(Number(capacity))) return
    onChange({ enabled, issuance_enabled: issuanceEnabled, max_tokens_per_user: Number(capacity) })
    setConfirmStop(false)
  }
  const confirmRevoke = () => {
    onRevokeUser()
    setConfirmRevokeUser(false)
  }
  return (
    <section className="grid gap-3 border-t border-[var(--hairline)] pt-4" aria-labelledby="access-token-admin-policy">
      <div><h3 id="access-token-admin-policy" className="text-sm font-semibold">{t('accessTokens.admin.title')}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('accessTokens.admin.hint')}</p></div>
      {loading ? <Skeleton className="h-16 rounded-md" /> : policy ? <div className="grid gap-2 sm:grid-cols-2">
        <div className="flex items-center justify-between gap-3 border-y border-[var(--hairline)] py-2"><span className="text-xs">{t('accessTokens.admin.enabled')}</span><Switch aria-label={t('accessTokens.admin.enabled')} size="sm" isDisabled={pending} isSelected={enabled} onValueChange={setEnabled} /></div>
        <div className="flex items-center justify-between gap-3 border-y border-[var(--hairline)] py-2"><span className="text-xs">{t('accessTokens.admin.issuanceEnabled')}</span><Switch aria-label={t('accessTokens.admin.issuanceEnabled')} size="sm" isDisabled={pending || !enabled} isSelected={issuanceEnabled} onValueChange={setIssuanceEnabled} /></div>
      </div> : <p role="alert" className="text-xs text-destructive">{t('accessTokens.errors.adminPolicy')}</p>}
      {policy ? <div className="flex flex-wrap items-end gap-3"><div className="grid gap-1.5"><label htmlFor="access-token-capacity" className="text-xs font-medium">{t('accessTokens.admin.maxTokens')}</label><Input id="access-token-capacity" size="sm" className="max-w-48" inputMode="numeric" isDisabled={pending} value={capacity} onValueChange={setCapacity} /></div><Button type="button" size="sm" color="primary" isDisabled={!changed || pending || !/^[1-9]\d*$/.test(capacity)} onPress={() => policy.enabled && !enabled ? setConfirmStop(true) : save()}>{t('accessTokens.admin.savePolicy')}</Button></div> : null}
      {error ? <p role="alert" className="text-xs text-destructive">{t('accessTokens.errors.adminAction')}</p> : null}
      {revokedCount !== undefined ? <p role="status" className="text-xs text-success">{t('accessTokens.admin.revokedCount', { count: revokedCount })}</p> : null}
      <div className="grid gap-2 border-t border-[var(--hairline)] pt-3 sm:grid-cols-[minmax(12rem,1fr)_auto] sm:items-end">
        <div className="grid gap-1.5"><label htmlFor="access-token-revoke-user" className="text-xs font-medium">{t('accessTokens.admin.userId')}</label><Input id="access-token-revoke-user" size="sm" inputMode="numeric" value={userId} onValueChange={onUserIdChange} /></div>
        <Button type="button" size="sm" color="danger" variant="flat" isDisabled={pending || !/^[1-9]\d*$/.test(userId)} onPress={() => setConfirmRevokeUser(true)}><Trash2 className="size-3.5" aria-hidden="true" />{t('accessTokens.admin.revokeAll')}</Button>
      </div>
      {confirmRevokeUser ? <p role="alert" className="text-xs text-warning">{t('accessTokens.admin.confirmRevokeBody', { userId })}<Button type="button" size="sm" variant="light" onPress={confirmRevoke}>{t('accessTokens.admin.revokeAll')}</Button><Button type="button" size="sm" variant="light" onPress={() => setConfirmRevokeUser(false)}>{t('accessTokens.actions.cancel')}</Button></p> : null}
      <p className="text-[11px] leading-4 text-muted-foreground">{t('accessTokens.admin.modelKeysIndependent')}</p>
      <AdminConfirmDialog open={confirmStop} pending={pending} title={t('accessTokens.admin.confirmDisableTitle')} body={t('accessTokens.admin.confirmDisableBody')} confirmLabel={t('accessTokens.admin.confirmDisable')} onClose={() => setConfirmStop(false)} onConfirm={save} />
    </section>
  )
}

function AdminConfirmDialog({ open, pending, title, body, confirmLabel, onClose, onConfirm }: { open: boolean; pending: boolean; title: string; body: string; confirmLabel: string; onClose: () => void; onConfirm: () => void }) {
  const { t } = useTranslation()
  return <Modal isOpen={open} isDismissable={!pending} onOpenChange={(isOpen) => !isOpen && !pending && onClose()} backdrop="blur"><ModalContent>{() => <>
    <ModalHeader className="flex flex-col gap-1"><h2 className="text-base font-semibold">{title}</h2><p className="text-sm font-normal leading-5 text-muted-foreground">{body}</p></ModalHeader>
    <ModalFooter><Button type="button" variant="light" isDisabled={pending} onPress={onClose}>{t('accessTokens.actions.cancel')}</Button><Button type="button" color="danger" variant="flat" isDisabled={pending} onPress={onConfirm}><TriangleAlert className="size-3.5" aria-hidden="true" />{confirmLabel}</Button></ModalFooter>
  </>}</ModalContent></Modal>
}

