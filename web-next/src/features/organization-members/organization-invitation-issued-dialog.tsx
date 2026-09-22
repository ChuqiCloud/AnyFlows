import { Button, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { Check, Copy } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

/** 邀请令牌只展示一次；对话框关闭后立即由父组件释放明文。 */
export function OrganizationInvitationIssuedDialog({
  organizationName,
  token,
  onClose,
}: {
  organizationName: string
  token?: string
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return undefined
    const timer = window.setTimeout(() => setCopied(false), 2_000)
    return () => window.clearTimeout(timer)
  }, [copied])
  const copy = async () => {
    if (!token) return
    try {
      await navigator.clipboard.writeText(token)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }
  return (
    <Modal backdrop="blur" hideCloseButton isDismissable={false} isOpen={token !== undefined} onOpenChange={(open) => { if (!open) onClose() }}>
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <h2 className="text-base font-semibold">{t('organizationConsole.invitations.issued.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('organizationConsole.invitations.issued.body', { organization: organizationName })}</p>
            </ModalHeader>
            <ModalBody className="gap-1.5">
              <div className="flex gap-2">
                <Input value={token ?? ''} isReadOnly size="sm" classNames={{ input: 'min-w-0 font-mono text-xs' }} />
                <Button type="button" isIconOnly size="md" variant="bordered" aria-label={t('organizationConsole.actions.copy')} onClick={() => void copy()}>
                  {copied ? <Check className="size-4 text-success" aria-hidden="true" /> : <Copy className="size-4" aria-hidden="true" />}
                </Button>
              </div>
              <p className="text-xs leading-5 text-muted-foreground">{t('organizationConsole.invitations.issued.hint')}</p>
            </ModalBody>
            <ModalFooter>
              <Button color="primary" onPress={onClose}>{t('organizationConsole.actions.done')}</Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
