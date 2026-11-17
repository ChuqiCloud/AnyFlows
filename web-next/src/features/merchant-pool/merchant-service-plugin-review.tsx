import { useState } from 'react'
import { Drawer, DrawerBody, DrawerContent, DrawerFooter, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import type { MerchantPluginReviewCheck } from './merchant-pool-model'
import { useMerchantPluginReviewers, useMerchantServicePluginReview } from './merchant-pool-api'
import { PoolButton as Button, PoolEmpty, PoolError, PoolField, PoolLoading, PoolSelect } from './merchant-pool-ui'
import { PluginUiViewsRenderer } from '@/features/plugin-lifecycle/plugin-ui-views'

function checkStyle(status: MerchantPluginReviewCheck['status']) {
  if (status === 'fail') return 'border-danger/25 bg-danger/5'
  if (status === 'warning') return 'border-warning/25 bg-warning/5'
  if (status === 'pass') return 'border-success/25 bg-success/5'
  return 'border-[var(--hairline)] bg-surface-2/40'
}

export function MerchantServicePluginReview({ serviceId, serviceName, onClose }: { serviceId: number; serviceName: string; onClose: () => void }) {
  const { t, i18n } = useTranslation()
  const reviewers = useMerchantPluginReviewers()
  const review = useMerchantServicePluginReview()
  const [selectedPluginId, setSelectedPluginId] = useState('')
  const plugins = reviewers.data?.plugins ?? []
  const pluginId = plugins.some((plugin) => plugin.plugin_id === selectedPluginId) ? selectedPluginId : plugins[0]?.plugin_id ?? ''
  const runReview = async () => {
    if (!pluginId) return
    try { await review.mutateAsync({ serviceId, pluginId }) } catch { /* Keep the selection so the merchant can retry. */ }
  }

  return <Drawer isOpen placement="right" size="2xl" scrollBehavior="inside" classNames={{ base: 'w-full max-h-none sm:max-w-xl' }} onOpenChange={(open) => { if (!open && !review.isPending) onClose() }}>
    <DrawerContent className="min-w-0 bg-surface-1 text-foreground">
      <DrawerHeader className="flex-col items-start gap-1">
        <h2>{t('merchantPool.pluginReview.title')}</h2>
        <p className="text-xs font-normal text-muted-foreground">{serviceName} · {t('merchantPool.pluginReview.subtitle')}</p>
      </DrawerHeader>
      <DrawerBody className="gap-4 pb-5">
        {reviewers.isPending ? <PoolLoading /> : reviewers.isError ? <PoolError message={t('merchantPool.pluginReview.pluginsError')} onRetry={() => void reviewers.refetch()} /> : plugins.length === 0 ? <PoolEmpty>{t('merchantPool.pluginReview.noPlugins')}</PoolEmpty> : <>
          <PoolField label={t('merchantPool.pluginReview.plugin')} hint={t('merchantPool.pluginReview.pluginHint')}>
            <PoolSelect label={t('merchantPool.pluginReview.plugin')} allowEmpty={false} value={pluginId} disabled={review.isPending} onChange={(value) => { setSelectedPluginId(value); review.reset() }} options={plugins.map((plugin) => ({ value: plugin.plugin_id, label: `${plugin.display_name} · ${plugin.plugin_version}` }))} />
          </PoolField>
          <div className="flex justify-end">
            <Button size="sm" color="primary" isDisabled={!pluginId || review.isPending} isLoading={review.isPending} onPress={() => void runReview()}>{t('merchantPool.pluginReview.run')}</Button>
          </div>
          {review.isError ? <PoolError message={t('merchantPool.pluginReview.runError')} onRetry={() => void runReview()} /> : null}
          {review.data ? <section className="grid gap-3" aria-live="polite">
            <div className="rounded-lg border border-[var(--hairline)] p-3">
              <div className="flex flex-wrap items-center justify-between gap-2 text-[11px] text-muted-foreground">
                <span>{review.data.display_name} · {review.data.plugin_id} · {review.data.plugin_version}</span>
                <span>{t('merchantPool.pluginReview.revision', { value: review.data.service_revision })}</span>
              </div>
              <p className="mt-2 whitespace-pre-wrap break-words text-sm leading-6">{review.data.summary}</p>
            </div>
            <h3 className="text-xs font-semibold">{t('merchantPool.pluginReview.checks', { count: review.data.checks.length })}</h3>
            {review.data.checks.length === 0 ? <PoolEmpty>{t('merchantPool.pluginReview.noChecks')}</PoolEmpty> : <ul className="grid gap-2">{review.data.checks.map((check, index) => <li key={`${check.code}-${index}`} className={`grid gap-1 rounded-lg border p-3 ${checkStyle(check.status)}`}>
              <div className="flex flex-wrap items-center gap-2 text-[11px]">
                <span className="font-medium">{t(`merchantPool.pluginReview.status.${check.status}`, { defaultValue: check.status })}</span>
                <code className="break-all text-muted-foreground">{check.code}</code>
              </div>
              <h4 className="text-xs font-semibold">{check.title}</h4>
              <p className="whitespace-pre-wrap break-words text-xs leading-5">{check.message}</p>
              {check.recommendation ? <p className="whitespace-pre-wrap break-words text-xs leading-5 text-muted-foreground">{t('merchantPool.pluginReview.recommendation')}: {check.recommendation}</p> : null}
            </li>)}</ul>}
            <PluginUiViewsRenderer manifest={{ uiViews: review.data.ui_views }} slot="merchant_service" locale={i18n.resolvedLanguage ?? 'en'} />
          </section> : null}
        </>}
      </DrawerBody>
      <DrawerFooter><Button size="sm" variant="flat" isDisabled={review.isPending} onPress={onClose}>{t('merchantPool.close')}</Button></DrawerFooter>
    </DrawerContent>
  </Drawer>
}
