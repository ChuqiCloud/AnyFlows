import { PaymentElement, useElements, useStripe } from '@stripe/react-stripe-js'
import type { PaymentIntent } from '@stripe/stripe-js'
import { CreditCard, LoaderCircle, LockKeyhole } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'

export function WalletStripePaymentForm({
  amountLabel,
  returnUrl,
  onConfirmed,
}: {
  amountLabel: string
  returnUrl: string
  onConfirmed: (status: PaymentIntent.Status) => Promise<void>
}) {
  const { t } = useTranslation()
  const stripe = useStripe()
  const elements = useElements()
  const [ready, setReady] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string>()

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!stripe || !elements || submitting) return

    setSubmitting(true)
    setError(undefined)
    try {
      const result = await stripe.confirmPayment({
        elements,
        confirmParams: { return_url: returnUrl },
        redirect: 'if_required',
      })
      if (result.error) {
        setError(result.error.message ?? t('wallet.topup.payment.errors.rejected'))
        return
      }
      if (!result.paymentIntent) {
        setError(t('wallet.topup.payment.errors.unknown'))
        return
      }
      await onConfirmed(result.paymentIntent.status)
    } catch {
      setError(t('wallet.topup.payment.errors.unknown'))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <form className="grid gap-4" onSubmit={(event) => void submit(event)}>
      <PaymentElement
        options={{ layout: 'tabs' }}
        onReady={() => setReady(true)}
      />
      {error ? (
        <p role="alert" className="text-xs leading-5 text-destructive">{error}</p>
      ) : null}
      <Button type="submit" className="w-full" disabled={!stripe || !elements || !ready || submitting}>
        {submitting ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <CreditCard aria-hidden="true" />}
        {t(submitting ? 'wallet.topup.payment.confirming' : 'wallet.topup.payment.pay', { amount: amountLabel })}
      </Button>
      <p className="flex items-center justify-center gap-1.5 text-[0.6875rem] text-muted-foreground">
        <LockKeyhole className="size-3" aria-hidden="true" />
        {t('wallet.topup.payment.securedByStripe')}
      </p>
    </form>
  )
}
