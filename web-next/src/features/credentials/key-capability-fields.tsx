import type { InputHTMLAttributes, ReactNode } from 'react'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'

export const capabilityInputClass = 'min-h-9 w-full min-w-0 rounded-lg border border-[var(--hairline)] bg-background px-2.5 py-1.5 text-xs disabled:opacity-50'
export function CapabilityField({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return <label className="grid min-w-0 content-start gap-1.5 text-xs"><span className="font-medium">{label}</span>{children}{hint ? <span className="text-[11px] leading-5 text-muted-foreground">{hint}</span> : null}</label>
}
export function CapabilityInput(props: InputHTMLAttributes<HTMLInputElement>) { return <input {...props} className={capabilityInputClass} /> }
export function CapabilityError({ code = 'error', onRetry }: { code?: string; onRetry?: () => void }) {
  const { t } = useTranslation()
  return <div role="alert" className="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-danger/10 px-3 py-2 text-xs text-danger"><span>{t(`keyCapabilities.${code}`, { defaultValue: t('keyCapabilities.error') })}</span>{onRetry ? <Button type="button" size="sm" variant="light" onPress={onRetry}>{t('keyCapabilities.retry')}</Button> : null}</div>
}
export function CapabilityTime({ value }: { value?: number | null }) {
  const { i18n, t } = useTranslation()
  return <>{value ? new Intl.DateTimeFormat(i18n.language, { dateStyle: 'medium', timeStyle: 'short' }).format(value * 1000) : t('keyCapabilities.unknown')}</>
}
export function parseModelNames(value: string) { return [...new Set(value.split(/\r?\n/).map((item) => item.trim()).filter(Boolean))] }
export function validModelNames(names: string[]) { return names.length <= 1000 && names.every((name) => new TextEncoder().encode(name).length <= 255 && !/[\u0000-\u001f\u007f]/.test(name)) }
export function nullableDecimal(value: string) { return value.trim() === '' ? null : value.trim() }
export function validDecimal(value: string, positive = false) { return value.trim() === '' || value.trim().length <= 64 && /^(?:0|[1-9]\d*)(?:\.\d{1,12})?$/.test(value.trim()) && (!positive || /[1-9]/.test(value)) }
export function validCurrency(value: string) { return value.trim() === '' || /^[A-Za-z0-9_]{1,16}$/.test(value.trim()) }
export function validInterval(value: string, minimum = 60, maximum = 604_800) { return value === '' || /^\d+$/.test(value) && Number.isSafeInteger(Number(value)) && Number(value) >= minimum && Number(value) <= maximum }
