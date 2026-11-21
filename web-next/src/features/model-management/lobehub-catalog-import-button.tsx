import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { useQueryClient } from '@tanstack/react-query'
import { Download } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { lobeHubModelCount, lobeHubProviders } from '@/components/brand/lobehub-providers'
import { adminModelMetadataQueryKey } from './model-management-api'
import { importLobeHubCatalog, type LobeHubImportProgress } from './lobehub-catalog-import'

export function LobeHubCatalogImportButton() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const controller = useRef<AbortController | null>(null)
  const [open, setOpen] = useState(false)
  const [pending, setPending] = useState(false)
  const [progress, setProgress] = useState<LobeHubImportProgress>()
  const [result, setResult] = useState<'complete' | 'stopped' | 'failed'>()
  useEffect(() => () => { controller.current?.abort() }, [])

  const run = async () => {
    if (controller.current) return
    const current = new AbortController()
    controller.current = current
    setPending(true)
    setProgress(undefined)
    setResult(undefined)
    try {
      await importLobeHubCatalog(current.signal, setProgress)
      setResult('complete')
    } catch {
      setResult(current.signal.aborted ? 'stopped' : 'failed')
    } finally {
      controller.current = null
      setPending(false)
      void Promise.all([
        queryClient.invalidateQueries({ queryKey: adminModelMetadataQueryKey }),
        queryClient.invalidateQueries({ queryKey: ['model-provider-catalog'] }),
        queryClient.invalidateQueries({ queryKey: ['admin-model-provider-catalog'] }),
      ])
    }
  }
  const completed = progress ? progress.modelsCreated + progress.providersCreated + progress.skipped : 0

  return (
    <>
      <Button type="button" size="sm" variant="bordered" onPress={() => { setResult(undefined); setProgress(undefined); setOpen(true) }}>
        <Download className="size-3.5" aria-hidden="true" />
        {t('modelManagement.lobehub.action')}
      </Button>
      <Modal isOpen={open} onOpenChange={setOpen} isDismissable={!pending} isKeyboardDismissDisabled={pending} hideCloseButton={pending}>
        <ModalContent>
          <ModalHeader>{t('modelManagement.lobehub.action')}</ModalHeader>
          <ModalBody className="text-sm">
            <p>{t('modelManagement.lobehub.description', { providers: lobeHubProviders.length, models: lobeHubModelCount })}</p>
            {progress ? (
              <div className="grid gap-1" role="status">
                <progress className="w-full" aria-label={t('modelManagement.lobehub.progressLabel')} value={completed} max={progress.total} />
                <p>{t('modelManagement.lobehub.progress', { ...progress, completed })}</p>
              </div>
            ) : pending ? <p role="status">{t('modelManagement.lobehub.loading')}</p> : null}
            {result ? <p role={result === 'failed' ? 'alert' : 'status'}>{t(`modelManagement.lobehub.${result}`)}</p> : null}
          </ModalBody>
          <ModalFooter>
            {pending ? (
              <Button size="sm" variant="flat" onPress={() => controller.current?.abort()}>{t('modelManagement.lobehub.stop')}</Button>
            ) : (
              <>
                <Button size="sm" variant="light" onPress={() => setOpen(false)}>{t('modelManagement.lobehub.close')}</Button>
                {result !== 'complete' ? <Button size="sm" color="primary" onPress={() => void run()}>{t('modelManagement.lobehub.submit')}</Button> : null}
              </>
            )}
          </ModalFooter>
        </ModalContent>
      </Modal>
    </>
  )
}
