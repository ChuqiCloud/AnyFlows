import { useTranslation } from 'react-i18next'
import { CheckCircle2, ShieldCheck } from 'lucide-react'
import { Button } from '@heroui/react'
import { useConsoleExtensionEntryPoint } from '@/extensions/query'
import { useAccountEligibility, type VerificationRecord } from './account-verification-api'
import { maskedVerificationName } from './verification-identity'

export function VerifiedAccount({ record, onReplace }: { record: VerificationRecord; onReplace: () => void }) {
  const { t } = useTranslation()
  const eligibility = useAccountEligibility()
  const entry = useConsoleExtensionEntryPoint('account-verification')
  const fields = [
    [t('verificationCenter.kind'), t(`verificationCenter.${record.kind}`)],
    [t('verificationCenter.subjectName'), maskedVerificationName(record.subject_name)],
    [t('verificationCenter.documentType'), t(`verificationCenter.documentTypes.${record.document_type}`, { defaultValue: record.document_type })],
    [t('verificationCenter.documentNumber'), record.document_number_masked || '—'],
    [t('verificationCenter.provider'), t(`verificationCenter.providers.${record.provider}`)],
    [t('verificationCenter.updated'), new Date(record.updated_at * 1000).toLocaleString()],
  ]
  return <section aria-label={t('verificationCenter.verifiedTitle')} data-verification-result className="overflow-hidden rounded-2xl border border-default-200 bg-content1">
    <div className="flex flex-wrap items-center gap-4 border-b border-default-200 bg-gradient-to-br from-success/10 via-success/5 to-transparent p-6 sm:p-8">
      <span className="grid size-16 shrink-0 place-items-center rounded-2xl border border-success/20 bg-success/10 text-success"><ShieldCheck className="size-8" aria-hidden="true" /></span>
      <div className="min-w-0 flex-1"><h3 className="flex items-center gap-2 text-lg font-semibold"><CheckCircle2 className="size-5 shrink-0 text-success" aria-hidden="true" />{t('verificationCenter.verifiedTitle')}</h3><p className="mt-2 text-sm leading-6 text-default-500">{t('verificationCenter.verifiedHint')}</p></div>
    </div>
    <dl className="grid gap-x-8 gap-y-6 p-6 sm:grid-cols-2 sm:p-8 lg:grid-cols-3">{fields.map(([label, value]) => <div key={label} className="min-w-0"><dt className="text-xs text-default-500">{label}</dt><dd className="mt-2 break-words text-sm font-medium">{value}</dd></div>)}</dl>
    <div className="flex flex-wrap items-center justify-between gap-3 border-t border-default-200 px-6 py-4 sm:px-8"><p className="text-xs leading-5 text-default-500">{t('verificationCenter.maskedHint')}</p><Button type="button" size="sm" variant="light" onClick={onReplace}>{t('verificationCenter.replace')}</Button></div>
    {eligibility.data?.enterprise_verified && entry ? <div className="flex flex-wrap items-center justify-between gap-3 border-t border-default-200 px-6 py-4 sm:px-8"><p className="text-sm text-default-500">{t('verificationCenter.enterpriseGranted')}</p><a className="text-sm font-medium text-primary hover:underline" href={entry.path}>{t('verificationCenter.applyWorkspace')}</a></div> : null}
  </section>
}
