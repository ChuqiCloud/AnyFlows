import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Textarea } from '@heroui/react'
import { Check, Copy, Download, TicketCheck, TriangleAlert } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { IssuedAdminRedemptionBatch } from '@/lib/api/generated/types.gen'
import { copyText } from '@/lib/clipboard'

type RedemptionIssuedDialogProps = {
  issued?: IssuedAdminRedemptionBatch
  onClose: () => void
}

export function RedemptionIssuedDialog({ issued, onClose }: RedemptionIssuedDialogProps) {
  const { t } = useTranslation()
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle')
  const codeText = useMemo(() => issued?.codes.join('\n') ?? '', [issued])

  useEffect(() => setCopyState('idle'), [issued?.batch.batch_id])

  const copy = async () => {
    if (!codeText) return
    setCopyState(await copyText(codeText) ? 'copied' : 'failed')
  }

  const download = () => {
    if (!issued || !codeText) return
    const url = URL.createObjectURL(new Blob([`${codeText}\n`], { type: 'text/plain;charset=utf-8' }))
    const link = document.createElement('a')
    link.href = url
    link.download = `redemption-codes-${issued.batch.batch_id}.txt`
    link.click()
    URL.revokeObjectURL(url)
  }

  return (
    <Modal
      backdrop="blur"
      classNames={{ base: 'max-h-[calc(100vh-2rem)]' }}
      hideCloseButton
      isDismissable={false}
      isOpen={issued !== undefined}
      scrollBehavior="inside"
      size="2xl"
      onOpenChange={(open) => !open && onClose()}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <div className="grid size-10 place-items-center rounded-lg bg-success/10 text-success">
                <TicketCheck className="size-4" aria-hidden="true" />
              </div>
              <h2 className="text-base font-semibold">{t('redemptions.issued.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">
                {t('redemptions.issued.description', {
                  count: issued?.codes.length ?? 0,
                  name: issued?.batch.name,
                })}
              </p>
            </ModalHeader>

            <ModalBody className="gap-3">
              <Textarea
                className="min-h-56"
                classNames={{ input: 'resize-y font-mono text-xs leading-5' }}
                isReadOnly
                spellCheck="false"
                value={codeText}
                onFocus={(event) => event.currentTarget.select()}
              />
              {copyState === 'failed' ? (
                <p role="alert" className="text-xs text-destructive">{t('redemptions.issued.copyFailed')}</p>
              ) : null}
              <p className="flex items-start gap-2 rounded-lg bg-warning/10 px-3 py-2 text-xs leading-5 text-warning">
                <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
                {t('redemptions.issued.warning')}
              </p>
            </ModalBody>

            <ModalFooter>
              <Button type="button" variant="bordered" onClick={download}>
                <Download className="size-4" aria-hidden="true" />
                {t('redemptions.issued.download')}
              </Button>
              <Button type="button" variant="bordered" onClick={() => void copy()}>
                {copyState === 'copied' ? <Check className="size-4" aria-hidden="true" /> : <Copy className="size-4" aria-hidden="true" />}
                {t(copyState === 'copied' ? 'redemptions.issued.copied' : 'redemptions.issued.copy')}
              </Button>
              <Button color="primary" onPress={onClose}>{t('redemptions.issued.done')}</Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
