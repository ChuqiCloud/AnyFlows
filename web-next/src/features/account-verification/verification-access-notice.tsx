import { ShieldAlert } from 'lucide-react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'

import { ApiError } from '@/lib/api'
import { useAccountEligibility } from './account-verification-api'

export type VerificationAccessScope = 'topup' | 'api'

export function useVerificationAccess(scope: VerificationAccessScope, error?: unknown) {
  const eligibility = useAccountEligibility()
  const required = scope === 'topup'
    ? eligibility.data?.require_verification_for_topup
    : eligibility.data?.require_verification_for_api
  return Boolean(!eligibility.data?.verified && (required || isVerificationRequiredError(error)))
}

export function isVerificationRequiredError(error: unknown) {
  return error instanceof ApiError
    && typeof error.details === 'object'
    && error.details !== null
    && 'code' in error.details
    && error.details.code === 'account_verification_required'
}

export function VerificationAccessNotice({ scope }: { scope: VerificationAccessScope }) {
  const { t } = useTranslation()
  return (
    <div role="alert" className="flex shrink-0 items-start gap-2 rounded-lg border border-warning/25 bg-warning/8 px-3 py-2 text-sm text-warning">
      <ShieldAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
      <div className="min-w-0">
        <p>{t(`verificationAccess.${scope}`)}</p>
        <Link className="mt-1 inline-block font-medium underline underline-offset-4" to="/console/account-verification">
          {t('verificationAccess.action')}
        </Link>
      </div>
    </div>
  )
}
