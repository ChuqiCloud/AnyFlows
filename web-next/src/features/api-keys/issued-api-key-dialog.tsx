import { Button, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { useEffect, useState } from 'react'
import { Check, Copy, KeyRound, MessageSquareText, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { IssuedUserToken } from '@/lib/api/generated/types.gen'
import { copyText } from '@/lib/clipboard'
import { navigateTo } from '@/lib/router-navigation'

type IssuedApiKeyDialogProps = {
  issued?: IssuedUserToken
  onClose: () => void
  showPlayground?: boolean
}

export function IssuedApiKeyDialog({ issued, onClose, showPlayground = true }: IssuedApiKeyDialogProps) {
  const { t } = useTranslation()
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle')

  useEffect(() => setCopyState('idle'), [issued?.token.id])

  const copy = async () => {
    if (!issued) return
    setCopyState(await copyText(issued.api_key) ? 'copied' : 'failed')
  }

  const openPlayground = () => {
    if (!issued) return
    onClose()
    navigateTo('/console/playground')
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
              <div className="grid size-10 place-items-center rounded-lg bg-success/10 text-success"><KeyRound className="size-4" aria-hidden="true" /></div>
              <h2 className="text-base font-semibold">{t('apiKeys.issued.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('apiKeys.issued.description', { name: issued?.token.name })}</p>
            </ModalHeader>
            <ModalBody className="gap-2">
              <div className="flex gap-2">
                <Input
                  className="min-w-0"
                  classNames={{ input: 'font-mono text-xs' }}
                  isReadOnly
                  size="sm"
                  value={issued?.api_key ?? ''}
                  onFocus={(event) => event.currentTarget.select()}
                />
                <Button type="button" className="shrink-0" color="primary" variant="flat" onClick={() => void copy()}>
                  {copyState === 'copied' ? <Check className="size-4" aria-hidden="true" /> : <Copy className="size-4" aria-hidden="true" />}
                  {t(copyState === 'copied' ? 'apiKeys.issued.copied' : 'apiKeys.issued.copy')}
                </Button>
              </div>
              {copyState === 'failed' ? <p role="alert" className="text-xs text-destructive">{t('apiKeys.issued.copyFailed')}</p> : null}
              <p className="flex items-start gap-2 rounded-lg bg-warning/10 px-3 py-2 text-xs leading-5 text-warning">
                <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
                {t('apiKeys.issued.warning')}
              </p>
            </ModalBody>
            <ModalFooter>
              {showPlayground ? (
                <Button type="button" variant="bordered" onClick={openPlayground}>
                  <MessageSquareText className="size-4" aria-hidden="true" />{t('apiKeys.issued.playground')}
                </Button>
              ) : null}
              <Button color="primary" onPress={onClose}>{t('apiKeys.issued.done')}</Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
