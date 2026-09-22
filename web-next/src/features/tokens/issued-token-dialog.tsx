import { useEffect, useState } from 'react'
import { Button, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { Check, Copy, KeyRound, MessageSquareText, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { IssuedAdminToken } from '@/lib/api/generated/types.gen'
import { navigateTo } from '@/lib/router-navigation'

type IssuedTokenDialogProps = {
  issued?: IssuedAdminToken
  onClose: () => void
}

export function IssuedTokenDialog({ issued, onClose }: IssuedTokenDialogProps) {
  const { t } = useTranslation()
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle')

  useEffect(() => setCopyState('idle'), [issued?.token.id])

  const copy = async () => {
    if (!issued) return
    try {
      await navigator.clipboard.writeText(issued.api_key)
      setCopyState('copied')
    } catch {
      setCopyState('failed')
    }
  }

  const openPlayground = () => {
    if (!issued) return
    onClose()
    navigateTo('/console/playground')
  }

  return (
    <Modal backdrop="blur" hideCloseButton isDismissable={false} isOpen={issued !== undefined} onOpenChange={(open) => !open && onClose()}>
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <div className="grid size-10 place-items-center rounded-lg bg-success/10 text-success"><KeyRound className="size-4" aria-hidden="true" /></div>
              <h2 className="text-base font-semibold">{t('tokens.issued.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('tokens.issued.description', { name: issued?.token.name })}</p>
            </ModalHeader>
            <ModalBody className="gap-2">
              <div className="flex gap-2">
                <Input className="min-w-0" classNames={{ input: 'font-mono text-xs' }} isReadOnly size="sm" value={issued?.api_key ?? ''} onFocus={(event) => event.target.select()} />
                <Button type="button" variant="bordered" className="shrink-0" onClick={copy}>
                  {copyState === 'copied' ? <Check className="size-3.5" aria-hidden="true" /> : <Copy className="size-3.5" aria-hidden="true" />}
                  {t(copyState === 'copied' ? 'tokens.issued.copied' : 'tokens.issued.copy')}
                </Button>
              </div>
              {copyState === 'failed' ? <p role="alert" className="text-xs text-destructive">{t('tokens.issued.copyFailed')}</p> : null}
              <p className="flex items-start gap-2 rounded-lg bg-warning/10 px-3 py-2 text-xs leading-5 text-warning">
                <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />{t('tokens.issued.warning')}
              </p>
            </ModalBody>
            <ModalFooter>
              <Button type="button" variant="bordered" onClick={openPlayground}>
                <MessageSquareText className="size-3.5" aria-hidden="true" />{t('tokens.issued.playground')}
              </Button>
              <Button color="primary" onPress={onClose}>{t('tokens.issued.done')}</Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
