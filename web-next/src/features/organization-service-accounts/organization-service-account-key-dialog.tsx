import { Button, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { Check, Copy, KeyRound } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { OrganizationServiceAccountIssuedResponse } from '@/lib/api/generated/types.gen'

export function OrganizationServiceAccountKeyDialog({
  issued,
  onClose,
}: {
  issued?: OrganizationServiceAccountIssuedResponse
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    setCopied(false)
  }, [issued])

  const copy = async () => {
    if (!issued) return
    try {
      await navigator.clipboard.writeText(issued.secret)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }

  return (
    <Modal
      backdrop="blur"
      hideCloseButton
      isDismissable={false}
      isOpen={issued !== undefined}
      onOpenChange={(open) => !open && onClose()}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <div className="grid size-10 place-items-center rounded-lg bg-success/10 text-success">
                <KeyRound className="size-4" aria-hidden="true" />
              </div>
              <h2 className="text-base font-semibold">{t('organizationServiceAccounts.keyDialog.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('organizationServiceAccounts.keyDialog.description')}</p>
            </ModalHeader>
            <ModalBody className="gap-3">
              {issued ? (
                <div className="grid gap-3">
                  <div className="grid gap-1.5">
                    <label htmlFor="organization-service-account-secret" className="text-xs font-medium">
                      {t('organizationServiceAccounts.keyDialog.secretLabel')}
                    </label>
                    <div className="flex gap-2">
                      <Input className="font-mono" classNames={{ input: 'text-xs' }} id="organization-service-account-secret" isReadOnly size="sm" value={issued.secret} />
                      <Button type="button" size="sm" variant="bordered" className="shrink-0" onClick={() => void copy()}>
                        {copied ? <Check className="size-3.5 text-success" aria-hidden="true" /> : <Copy className="size-3.5" aria-hidden="true" />}
                        {t(copied ? 'organizationServiceAccounts.keyDialog.copied' : 'organizationServiceAccounts.keyDialog.copy')}
                      </Button>
                    </div>
                  </div>
                  <dl className="grid grid-cols-2 gap-3 rounded-lg border border-[var(--hairline)] bg-surface-2/45 p-3 text-xs">
                    <div>
                      <dt className="text-muted-foreground">{t('organizationServiceAccounts.keyDialog.prefix')}</dt>
                      <dd className="mt-1 font-mono font-medium">{issued.key_prefix}</dd>
                    </div>
                    <div>
                      <dt className="text-muted-foreground">{t('organizationServiceAccounts.keyDialog.keyId')}</dt>
                      <dd className="mt-1 font-mono font-medium">{issued.key_key}</dd>
                    </div>
                  </dl>
                  <p className="rounded-lg bg-warning/8 px-3 py-2 text-xs leading-5 text-warning">
                    {t('organizationServiceAccounts.keyDialog.warning')}
                  </p>
                </div>
              ) : null}
            </ModalBody>
            <ModalFooter>
              <Button color="primary" onPress={onClose}>{t('organizationServiceAccounts.actions.done')}</Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
