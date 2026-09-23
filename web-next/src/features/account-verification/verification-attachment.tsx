import { useEffect, useRef, useState } from 'react'
import { Download, Eye, LoaderCircle, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@heroui/react'

type Props = { name: string; contentType: string; size: number; file?: File; load?: (signal: AbortSignal) => Promise<Blob> }

/** Material URLs exist only while this card is mounted; authenticated bytes are never persisted. */
export function VerificationAttachment({ name, contentType, size, file, load }: Props) {
  const { t } = useTranslation()
  const [url, setUrl] = useState<string>()
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState(false)
  const current = useRef<string | undefined>(undefined)
  const pending = useRef<AbortController | undefined>(undefined)
  const alive = useRef(true)
  const previewable = ['image/png', 'image/jpeg', 'image/webp', 'application/pdf'].includes(contentType)

  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
      pending.current?.abort()
      if (current.current) URL.revokeObjectURL(current.current)
    }
  }, [])
  useEffect(() => {
    pending.current?.abort()
    if (current.current) URL.revokeObjectURL(current.current)
    current.current = file && previewable ? URL.createObjectURL(file) : undefined
    setUrl(current.current)
    setLoading(false)
    setError(false)
    return () => { if (current.current) URL.revokeObjectURL(current.current) }
  }, [file, name, contentType, previewable])

  const open = async (download = false) => {
    if (loading) return
    setLoading(true)
    setError(false)
    const controller = new AbortController()
    pending.current = controller
    try {
      let objectUrl = current.current
      if (!objectUrl) {
        const blob = file ?? await load?.(controller.signal)
        if (!blob || !alive.current || controller.signal.aborted) return
        // Do not trust an arbitrary declared MIME type to render active content.
        if (!previewable) throw new Error('Unsupported material type')
        objectUrl = URL.createObjectURL(new Blob([blob], { type: contentType }))
        current.current = objectUrl
      }
      if (download) {
        const anchor = document.createElement('a')
        anchor.href = objectUrl
        anchor.download = name
        anchor.click()
      } else setUrl(objectUrl)
    } catch {
      if (!controller.signal.aborted && alive.current) setError(true)
    } finally {
      if (alive.current && !controller.signal.aborted) setLoading(false)
    }
  }
  const close = () => {
    if (current.current) URL.revokeObjectURL(current.current)
    current.current = undefined
    setUrl(undefined)
  }
  return <article className="min-w-0 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div className="min-w-0"><p className="truncate text-xs font-medium" title={name}>{name}</p><p className="text-xs text-muted-foreground">{contentType} · {Math.ceil(size / 1024)} KB</p></div>
      <div className="flex gap-1">
        <Button type="button" size="sm" variant="bordered" isDisabled={loading || !previewable} onClick={() => url ? close() : void open()}>{loading ? <LoaderCircle className="size-4 animate-spin" /> : url ? <X className="size-4" /> : <Eye className="size-4" />}{t(url ? 'verificationCenter.hidePreview' : 'verificationCenter.preview')}</Button>
        <Button type="button" size="sm" variant="light" isDisabled={loading} onClick={() => void open(true)}><Download className="size-4" />{t('verificationCenter.download')}</Button>
      </div>
    </div>
    {error ? <p role="alert" className="mt-2 text-xs text-destructive">{t('verificationCenter.materialError')}</p> : null}
    {url ? contentType === 'application/pdf'
      ? <iframe src={url} title={name} sandbox="" referrerPolicy="no-referrer" className="mt-3 h-96 w-full rounded border-0" />
      : <a href={url} target="_blank" rel="noopener noreferrer"><img src={url} alt={name} className="mt-3 max-h-72 w-full rounded object-contain" loading="lazy" /></a>
      : null}
    {url && contentType === 'application/pdf' ? <p className="mt-2 text-xs text-muted-foreground">{t('verificationCenter.pdfHint')}</p> : null}
  </article>
}
