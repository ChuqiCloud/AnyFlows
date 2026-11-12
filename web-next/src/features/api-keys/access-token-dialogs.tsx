import { Button, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { Check, Copy, KeyRound, Trash2, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { AccessToken } from './access-token-api'

export function IssuedAccessTokenDialog({ issued, copyState, onCopy, onClose }: { issued?: { token: AccessToken; secret: string }; copyState: 'idle' | 'copied' | 'failed'; onCopy: () => Promise<void>; onClose: () => void }) {
  const { t } = useTranslation()
  return <Modal isOpen={issued !== undefined} isDismissable={false} hideCloseButton backdrop="blur" onOpenChange={(open) => !open && onClose()}><ModalContent>{() => <>
    <ModalHeader className="flex flex-col gap-1"><div className="grid size-9 place-items-center rounded-md bg-success/10 text-success"><KeyRound className="size-4" aria-hidden="true" /></div><h2 className="mt-2 text-base font-semibold">{t('accessTokens.issued.title')}</h2><p className="text-xs font-normal text-muted-foreground">{t('accessTokens.issued.description', { name: issued?.token.name })}</p></ModalHeader>
    <ModalBody><div className="flex gap-2"><Input aria-label={t('accessTokens.issued.secret')} classNames={{ input: 'font-mono text-xs' }} isReadOnly size="sm" value={issued?.secret ?? ''} onFocus={(event) => event.currentTarget.select()} /><Button type="button" size="sm" variant="flat" onPress={() => void onCopy()}>{copyState === 'copied' ? <Check className="size-3.5" aria-hidden="true" /> : <Copy className="size-3.5" aria-hidden="true" />}{t(copyState === 'copied' ? 'accessTokens.issued.copied' : 'accessTokens.issued.copy')}</Button></div>
      {copyState === 'failed' ? <p role="alert" className="text-xs text-destructive">{t('accessTokens.issued.copyFailed')}</p> : null}<p className="flex items-start gap-2 rounded-md bg-warning/10 px-3 py-2 text-xs leading-5 text-warning"><TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />{t('accessTokens.issued.warning')}</p>
      <div className="grid gap-1.5"><p className="text-xs font-medium">{t('accessTokens.issued.exampleTitle')}</p><pre className="overflow-x-auto rounded-md bg-surface-2 p-3 text-[11px] leading-5"><code>{'curl "$ANYFLOWS_BASE_URL/api/account/wallet" \\\n  -H "Authorization: Bearer $ANYFLOWS_ACCESS_TOKEN"'}</code></pre><p className="text-[11px] leading-4 text-muted-foreground">{t('accessTokens.issued.exampleHint')}</p></div>
    </ModalBody>
    <ModalFooter><Button color="primary" onPress={onClose}>{t('accessTokens.issued.done')}</Button></ModalFooter>
  </>}</ModalContent></Modal>
}

export function RevokeAccessTokenDialog({ token, pending, failed, onClose, onConfirm }: { token?: AccessToken; pending: boolean; failed: boolean; onClose: () => void; onConfirm: () => void }) {
  const { t } = useTranslation()
  return <Modal isOpen={token !== undefined} isDismissable={!pending} onOpenChange={(open) => !open && !pending && onClose()} backdrop="blur"><ModalContent>{() => <>
    <ModalHeader className="flex flex-col gap-1"><h2 className="text-base font-semibold">{t('accessTokens.revoke.title')}</h2><p className="text-sm font-normal leading-5 text-muted-foreground">{t('accessTokens.revoke.description', { name: token?.name })}</p></ModalHeader>
    <ModalBody>{failed ? <p role="alert" className="text-xs text-destructive">{t('accessTokens.errors.revoke')}</p> : null}</ModalBody>
    <ModalFooter><Button variant="light" isDisabled={pending} onPress={onClose}>{t('accessTokens.actions.cancel')}</Button><Button color="danger" variant="flat" isDisabled={pending} onPress={onConfirm}><Trash2 className="size-3.5" aria-hidden="true" />{t('accessTokens.actions.revoke')}</Button></ModalFooter>
  </>}</ModalContent></Modal>
}

export function RevokeAllAccessTokensDialog({ open, pending, failed, onClose, onConfirm }: { open: boolean; pending: boolean; failed: boolean; onClose: () => void; onConfirm: () => void }) {
  const { t } = useTranslation()
  return <Modal isOpen={open} isDismissable={!pending} onOpenChange={(isOpen) => !isOpen && !pending && onClose()} backdrop="blur"><ModalContent>{() => <>
    <ModalHeader className="flex flex-col gap-1"><h2 className="text-base font-semibold">{t('accessTokens.revoke.allTitle')}</h2><p className="text-sm font-normal leading-5 text-muted-foreground">{t('accessTokens.revoke.allDescription')}</p></ModalHeader>
    <ModalBody>{failed ? <p role="alert" className="text-xs text-destructive">{t('accessTokens.errors.revoke')}</p> : null}</ModalBody>
    <ModalFooter><Button variant="light" isDisabled={pending} onPress={onClose}>{t('accessTokens.actions.cancel')}</Button><Button color="danger" variant="flat" isDisabled={pending} onPress={onConfirm}><Trash2 className="size-3.5" aria-hidden="true" />{t('accessTokens.actions.revokeAll')}</Button></ModalFooter>
  </>}</ModalContent></Modal>
}
