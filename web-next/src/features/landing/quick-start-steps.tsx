import {
  ArrowRight,
  Boxes,
  Check,
  Code2,
  Copy,
  KeyRound,
  Sparkles,
  Terminal,
} from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Link as RouterLink } from 'react-router-dom'

import { copyText } from '@/lib/clipboard'
import { useLandingBaseUrl } from './use-landing-models'

export function QuickStartSteps() {
  const { t } = useTranslation()
  const baseUrl = useLandingBaseUrl()
  const [copiedKey, setCopiedKey] = useState(false)

  const handleCopySampleKey = async () => {
    const success = await copyText(`export OPENAI_BASE_URL="${baseUrl}/v1"\nexport OPENAI_API_KEY="YOUR_API_KEY"`)
    if (success) {
      setCopiedKey(true)
      setTimeout(() => setCopiedKey(false), 2000)
    }
  }

  return (
    <section className="border-t border-[var(--hairline)] bg-surface-sunken/40 py-20 sm:py-24">
      <div className="mx-auto w-full max-w-[1240px] px-4 sm:px-6 lg:px-8">
        {/* Header */}
        <div className="flex flex-col sm:flex-row sm:items-end justify-between gap-6">
          <div>
            <div className="inline-flex items-center gap-1.5 font-mono text-xs font-semibold uppercase tracking-[0.2em] text-primary">
              <Sparkles className="size-3.5" />
              <span>{t('landing.quickStart.eyebrow')}</span>
            </div>
            <h2 className="mt-3 text-3xl font-bold tracking-tight text-foreground sm:text-4xl">
              {t('landing.quickStart.title')}
            </h2>
            <p className="mt-2.5 max-w-2xl text-sm leading-6 text-muted-foreground sm:text-base">
              {t('landing.quickStart.description')}
            </p>
          </div>

          <RouterLink
            to="/api"
            className="inline-flex items-center gap-2 text-sm font-semibold text-primary hover:text-primary/80 transition-colors shrink-0 group"
          >
            <span>{t('landing.quickStart.apiDocs')}</span>
            <ArrowRight className="size-4 transition-transform group-hover:translate-x-1" />
          </RouterLink>
        </div>

        {/* 4 Steps Grid */}
        <div className="mt-12 grid gap-6 md:grid-cols-2 lg:grid-cols-4">
          {/* STEP 01 */}
          <div className="relative flex flex-col justify-between rounded-2xl border border-[var(--hairline)] bg-surface-1 p-6 shadow-xs transition-all hover:border-primary/40 hover:-translate-y-1">
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-xs font-semibold text-primary tracking-wider">
                  {t('landing.quickStart.step', { number: '01' })}
                </span>
                <Boxes className="size-4 text-muted-foreground" />
              </div>
              <h3 className="mt-4 text-base font-semibold text-foreground">
                {t('landing.quickStart.models.title')}
              </h3>
              <p className="mt-2 text-xs leading-5 text-muted-foreground">
                {t('landing.quickStart.models.description')}
              </p>
            </div>

            <div className="mt-6 rounded-xl border border-[var(--hairline)] bg-surface-sunken p-3 text-xs">
              <div className="flex items-center justify-between text-muted-foreground text-[0.6875rem]">
                <span>{t('landing.quickStart.models.catalog')}</span>
                <span className="text-success font-mono font-medium">{t('landing.quickStart.models.details')}</span>
              </div>
              <div className="mt-2 flex items-center justify-between font-mono text-[0.6875rem]">
                <span className="text-foreground">{t('landing.quickStart.models.pricing')}</span>
                <span className="text-primary font-medium">{t('landing.quickStart.models.browse')}</span>
              </div>
            </div>
          </div>

          {/* STEP 02 */}
          <div className="relative flex flex-col justify-between rounded-2xl border border-[var(--hairline)] bg-surface-1 p-6 shadow-xs transition-all hover:border-primary/40 hover:-translate-y-1">
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-xs font-semibold text-primary tracking-wider">
                  {t('landing.quickStart.step', { number: '02' })}
                </span>
                <KeyRound className="size-4 text-muted-foreground" />
              </div>
              <h3 className="mt-4 text-base font-semibold text-foreground">
                {t('landing.quickStart.key.title')}
              </h3>
              <p className="mt-2 text-xs leading-5 text-muted-foreground">
                {t('landing.quickStart.key.description')}
              </p>
            </div>

            <div className="mt-6 rounded-xl border border-[var(--hairline)] bg-surface-sunken p-3 font-mono text-[0.6875rem]">
              <div className="text-muted-foreground"># {t('landing.quickStart.key.ready')}</div>
              <div className="mt-1 text-success truncate font-semibold">
                sk-anyflows-9x7c••••••••
              </div>
              <div className="mt-1 text-muted-foreground text-[0.625rem]">
                {t('landing.quickStart.key.billing')}
              </div>
            </div>
          </div>

          {/* STEP 03 */}
          <div className="relative flex flex-col justify-between rounded-2xl border border-[var(--hairline)] bg-surface-1 p-6 shadow-xs transition-all hover:border-primary/40 hover:-translate-y-1">
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-xs font-semibold text-primary tracking-wider">
                  {t('landing.quickStart.step', { number: '03' })}
                </span>
                <Code2 className="size-4 text-muted-foreground" />
              </div>
              <h3 className="mt-4 text-base font-semibold text-foreground">
                {t('landing.quickStart.client.title')}
              </h3>
              <p className="mt-2 text-xs leading-5 text-muted-foreground">
                {t('landing.quickStart.client.description')}
              </p>
            </div>

            <div className="mt-6 flex flex-wrap gap-1.5 pt-1">
              <span className="rounded-md border border-[var(--hairline)] bg-surface-sunken px-2 py-1 text-[0.6875rem] font-medium text-foreground">
                Claude Code
              </span>
              <span className="rounded-md border border-[var(--hairline)] bg-surface-sunken px-2 py-1 text-[0.6875rem] font-medium text-foreground">
                Cursor
              </span>
              <span className="rounded-md border border-[var(--hairline)] bg-surface-sunken px-2 py-1 text-[0.6875rem] font-medium text-foreground">
                Cline
              </span>
              <span className="rounded-md border border-[var(--hairline)] bg-surface-sunken px-2 py-1 text-[0.6875rem] font-medium text-foreground">
                OpenAI SDK
              </span>
            </div>
          </div>

          {/* STEP 04 */}
          <div className="relative flex flex-col justify-between rounded-2xl border border-primary/30 bg-gradient-to-b from-primary/8 via-surface-1 to-surface-1 p-6 shadow-xs transition-all hover:border-primary/60 hover:-translate-y-1">
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-xs font-semibold text-primary tracking-wider">
                  {t('landing.quickStart.step', { number: '04' })}
                </span>
                <Terminal className="size-4 text-primary" />
              </div>
              <h3 className="mt-4 text-base font-semibold text-foreground">
                {t('landing.quickStart.endpoint.title')}
              </h3>
              <p className="mt-2 text-xs leading-5 text-muted-foreground">
                {t('landing.quickStart.endpoint.description')}
              </p>
            </div>

            <div className="mt-6">
              <button
                type="button"
                onClick={handleCopySampleKey}
                aria-label={copiedKey ? t('landing.home.terminal.copied') : t('landing.home.terminal.copy')}
                className="w-full flex items-center justify-between rounded-xl border border-[var(--hairline)] bg-surface-sunken p-2.5 font-mono text-[0.6875rem] text-muted-foreground hover:text-foreground hover:border-primary/40 transition-colors cursor-pointer"
              >
                <span className="truncate">export OPENAI_BASE_URL=...</span>
                {copiedKey ? (
                  <Check className="size-3.5 text-success shrink-0" />
                ) : (
                  <Copy className="size-3.5 shrink-0" />
                )}
              </button>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
