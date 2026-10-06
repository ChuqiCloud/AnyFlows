import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { CheckCircle2, Clock3, QrCode, Smartphone } from 'lucide-react'
import { Button } from '@/components/ui/button'
import type { QRCodeSVG } from '@rc-component/qrcode'
import type { VerificationRecord } from './account-verification-api'
import { alipayAuthorizationUrl, alipayLaunchUrl, alipayRemainingSeconds, isAlipayMobile } from './alipay-authorization'

function LocalQr({ value }: { value: string }) {
  const { t } = useTranslation()
  const [Qr, setQr] = useState<typeof QRCodeSVG>()
  const [failed, setFailed] = useState(false)
  const [attempt, setAttempt] = useState(0)
  useEffect(() => {
    let cancelled = false
    setFailed(false)
    void import('@rc-component/qrcode').then((module) => {
      if (!cancelled) setQr(() => module.QRCodeSVG)
    }).catch(() => { if (!cancelled) setFailed(true) })
    return () => { cancelled = true }
  }, [attempt])
  return <div className="flex aspect-square w-full max-w-64 items-center justify-center rounded-xl bg-white p-3 text-black">
    {Qr ? <Qr value={value} size={232} level="M" marginSize={4} bgColor="#ffffff" fgColor="#111827" style={{ width: '100%', height: 'auto' }} title={t('verificationCenter.alipayScan.qrLabel')} />
      : failed ? <div className="grid gap-3 text-center text-sm"><p role="alert">{t('verificationCenter.alipayScan.qrError')}</p><Button type="button" variant="secondary" onClick={() => setAttempt((value) => value + 1)}>{t('verificationCenter.refresh')}</Button></div>
        : <p role="status" className="text-sm">{t('verificationCenter.loading')}</p>}
  </div>
}

export function AlipayVerification({ record, receivedAt, checking, checkError, onCheck, onRestart }: {
  record: VerificationRecord
  receivedAt: number
  checking: boolean
  checkError: boolean
  onCheck: () => void
  onRestart: () => void
}) {
  const { t } = useTranslation()
  const [now, setNow] = useState(Date.now)
  const [mobile] = useState(() => isAlipayMobile(navigator.userAgent))
  const [showQr, setShowQr] = useState(!mobile)
  const attempted = useRef(false)
  const remaining = alipayRemainingSeconds(record, receivedAt, now)
  const url = alipayAuthorizationUrl(record.provider_action_url)
  const active = remaining > 0 && url !== null
  const launch = () => {
    if (alipayRemainingSeconds(record, receivedAt, Date.now()) <= 0 || !url) return
    const target = alipayLaunchUrl(url, navigator.userAgent)
    if (target) { try { window.location.assign(target) } catch { setShowQr(true) } }
  }
  useEffect(() => {
    if (!active) return
    const tick = () => setNow(Date.now())
    const interval = window.setInterval(tick, 1000)
    document.addEventListener('visibilitychange', tick)
    return () => { window.clearInterval(interval); document.removeEventListener('visibilitychange', tick) }
  }, [active])
  useEffect(() => {
    if (!mobile || !active) return
    const fallback = window.setTimeout(() => setShowQr(true), 1500)
    if (!attempted.current) {
      attempted.current = true
      const target = alipayLaunchUrl(url!, navigator.userAgent)
      if (target) { try { window.location.assign(target) } catch { setShowQr(true) } }
    }
    return () => window.clearTimeout(fallback)
  }, [mobile, active, url])

  const completed = record.status === 4
  const expired = record.provider_status === 'expired' || (record.status === 1 && remaining === 0)
  return <section aria-label={t('verificationCenter.alipayScan.title')} className="grid gap-5 rounded-xl border border-[var(--hairline)] bg-surface-1 p-4 sm:p-5">
    <div className="flex items-start gap-3"><span className="rounded-lg bg-brand/10 p-2 text-brand">{completed ? <CheckCircle2 className="size-5" /> : <QrCode className="size-5" />}</span><div className="min-w-0"><h4 className="font-semibold">{t(completed ? 'verificationCenter.alipayScan.completed' : 'verificationCenter.alipayScan.title')}</h4><p className="mt-1 text-sm text-muted-foreground">{t(completed ? 'verificationCenter.alipayScan.completedHint' : active ? 'verificationCenter.alipayScan.instructions' : expired ? 'verificationCenter.alipayScan.expiredHint' : 'verificationCenter.alipayScan.unavailable')}</p></div></div>
    {active ? <div className="grid items-center gap-5 sm:grid-cols-[minmax(0,16rem)_1fr]">
      {showQr ? <div className="grid justify-items-center gap-2"><LocalQr value={url!} /><p className="text-xs text-muted-foreground">{t('verificationCenter.alipayScan.privateHint')}</p></div> : <div role="status" className="flex min-h-40 items-center justify-center gap-2 text-sm text-muted-foreground"><Smartphone className="size-5" />{t('verificationCenter.alipayScan.opening')}</div>}
      <div className="grid gap-4"><p className="flex items-center gap-2 text-sm"><Clock3 className="size-4 text-muted-foreground" /><span>{t('verificationCenter.alipayScan.expiresIn', { time: `${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, '0')}` })}</span></p>
        {mobile ? <><Button type="button" onClick={launch}><Smartphone className="size-4" />{t('verificationCenter.alipayScan.openApp')}</Button><p className="text-xs leading-relaxed text-muted-foreground">{t('verificationCenter.alipayScan.fallbackHint')}</p>{!showQr ? <Button type="button" variant="secondary" onClick={() => setShowQr(true)}>{t('verificationCenter.alipayScan.showQr')}</Button> : null}</> : null}
        <Button type="button" variant="secondary" disabled={checking} onClick={onCheck}>{t('verificationCenter.alipayScan.checkResult')}</Button><p className="text-xs text-muted-foreground">{t('verificationCenter.alipayScan.autoRefresh')}</p>
      </div>
    </div> : !completed ? <div className="flex flex-wrap gap-2"><Button type="button" onClick={onRestart}>{t('verificationCenter.alipayScan.restart')}</Button><Button type="button" variant="secondary" disabled={checking} onClick={onCheck}>{t('verificationCenter.alipayScan.checkResult')}</Button></div> : null}
    {checkError ? <p role="alert" className="text-sm text-destructive">{t('verificationCenter.alipayScan.checkError')}</p> : null}
  </section>
}
