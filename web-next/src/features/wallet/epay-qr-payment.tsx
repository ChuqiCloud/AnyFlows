import { Button } from '@heroui/react'
import type { QRCodeSVG } from '@rc-component/qrcode'
import { ExternalLink, QrCode, RefreshCw, Smartphone } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

export function EpayQrPayment({ url, paymentMethod, refreshing, onRefresh }: {
  url: string
  paymentMethod: 'alipay' | 'wxpay'
  refreshing: boolean
  onRefresh: () => void
}) {
  const { t } = useTranslation()
  const [Qr, setQr] = useState<typeof QRCodeSVG>()
  const [qrError, setQrError] = useState(false)
  const [retry, setRetry] = useState(0)
  const [showQr, setShowQr] = useState(true)
  const launchedUrl = useRef<string | undefined>(undefined)
  const mobileAlipay = paymentMethod === 'alipay' && /Android|iPhone|iPad|iPod|AlipayClient/i.test(navigator.userAgent)

  useEffect(() => {
    let cancelled = false
    setQr(undefined)
    setQrError(false)
    void import('@rc-component/qrcode').then((module) => {
      if (!cancelled) setQr(() => module.QRCodeSVG)
    }).catch(() => { if (!cancelled) setQrError(true) })
    return () => { cancelled = true }
  }, [retry])

  useEffect(() => {
    if (!mobileAlipay) return
    const timer = window.setTimeout(() => setShowQr(true), 1_500)
    if (launchedUrl.current !== url) {
      launchedUrl.current = url
      setShowQr(false)
      try {
        window.location.assign(alipayAppUrl(url))
      } catch {
        setShowQr(true)
      }
    }
    return () => window.clearTimeout(timer)
  }, [mobileAlipay, url])

  return (
    <div className="grid justify-items-center gap-3 py-2 text-center">
      <div className="flex items-center gap-2 text-sm font-medium"><QrCode className="size-4 text-brand" aria-hidden="true" />{t('epayQr.title')}</div>
      {showQr ? (
        <div className="flex aspect-square w-full max-w-56 items-center justify-center rounded-lg bg-white p-2 text-black">
          {Qr ? <Qr value={url} size={208} level="L" marginSize={4} bgColor="#ffffff" fgColor="#111827" style={{ width: '100%', height: 'auto' }} title={t('epayQr.title')} />
            : qrError ? <Button type="button" size="sm" variant="bordered" onClick={() => setRetry((value) => value + 1)}>{t('epayQr.retryQr')}</Button>
              : <span role="status" className="text-xs">{t('epayQr.loading')}</span>}
        </div>
      ) : <div role="status" className="flex min-h-40 items-center gap-2 text-sm text-muted-foreground"><Smartphone className="size-4" />{t('epayQr.opening')}</div>}
      <p className="max-w-xs text-xs leading-5 text-muted-foreground">{t('epayQr.hint')}</p>
      <div className="flex flex-wrap justify-center gap-2">
        {mobileAlipay ? <Button type="button" size="sm" color="primary" onClick={() => { try { window.location.assign(alipayAppUrl(url)) } catch { setShowQr(true) } }}><Smartphone className="size-3.5" aria-hidden="true" />{t('epayQr.openAlipay')}</Button> : null}
        <Button type="button" size="sm" variant="bordered" onClick={() => window.location.assign(url)}><ExternalLink className="size-3.5" aria-hidden="true" />{t('epayQr.openPayment')}</Button>
        <Button type="button" size="sm" variant="light" isDisabled={refreshing} onClick={onRefresh}><RefreshCw className={refreshing ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />{t('epayQr.refreshStatus')}</Button>
      </div>
    </div>
  )
}

function alipayAppUrl(url: string) {
  return /AlipayClient/i.test(navigator.userAgent)
    ? url
    : `alipays://platformapi/startapp?appId=20000067&url=${encodeURIComponent(url)}`
}
