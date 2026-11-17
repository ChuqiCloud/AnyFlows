import { useMemo, useState } from 'react'
import { Button, Checkbox, Chip, Drawer, DrawerBody, DrawerContent, DrawerFooter, DrawerHeader, Select, SelectItem, Skeleton } from '@heroui/react'
import { useTranslation } from 'react-i18next'

import type { ExternalAccountSnapshotReviewCheck } from './external-accounts-api'
import { useExternalAccountSnapshotReviewers, useReviewExternalAccountSnapshots } from './external-accounts-api'
import { PluginUiViewsRenderer } from '@/features/plugin-lifecycle/plugin-ui-views'

function checkStyle(status: ExternalAccountSnapshotReviewCheck['status']) {
  if (status === 'fail') return 'border-danger/25 bg-danger/5'
  if (status === 'warning') return 'border-warning/25 bg-warning/5'
  if (status === 'pass') return 'border-success/25 bg-success/5'
  return 'border-[var(--hairline)] bg-default-50'
}

function observedAt(value: number, locale: string) {
  return new Date(value * 1000).toLocaleString(locale)
}

/** The caller explicitly selects and starts a plugin; returned strings are rendered as text only. */
export function ExternalAccountSnapshotPluginReview({ open, connectionId, onClose }: {
  open: boolean
  connectionId: number
  onClose: () => void
}) {
  const { t, i18n } = useTranslation()
  const reviewers = useExternalAccountSnapshotReviewers(open)
  const review = useReviewExternalAccountSnapshots()
  const [selection, setSelection] = useState('')
  const [allowExternalReads, setAllowExternalReads] = useState(false)
  const plugins = reviewers.data?.plugins ?? []
  const selectedPluginId = useMemo(() =>
    plugins.some((plugin) => plugin.pluginId === selection) ? selection : '',
  [plugins, selection])
  const selectedPlugin = plugins.find((plugin) => plugin.pluginId === selectedPluginId)

  const runReview = async () => {
    if (!selectedPluginId || review.isPending) return
    try {
      await review.mutateAsync({
        connectionId,
        pluginId: selectedPluginId,
        allowExternalReads: allowExternalReads && selectedPlugin?.supportsExternalReads === true,
      })
    } catch {
      // Keep the selected plugin so the user can retry after reviewing the error.
    } finally {
      setAllowExternalReads(false)
    }
  }

  const locale = i18n.resolvedLanguage?.startsWith('zh') ? 'zh-CN' : 'en-US'
  return <Drawer isOpen={open} placement="right" size="2xl" scrollBehavior="inside" onOpenChange={(next) => { if (!next && !review.isPending) { setAllowExternalReads(false); onClose() } }}>
    <DrawerContent className="min-w-0 bg-content1 text-foreground sm:max-w-xl">
      <DrawerHeader className="flex-col items-start gap-1">
        <h2>{t('profile.externalAccounts.pluginReview.title')}</h2>
        <p className="text-xs font-normal text-muted-foreground">{t('profile.externalAccounts.pluginReview.subtitle')}</p>
      </DrawerHeader>
      <DrawerBody className="gap-4 pb-5">
        <div className="rounded-lg border border-warning/25 bg-warning/5 p-3 text-xs leading-5">
          {t('profile.externalAccounts.pluginReview.consent')}
        </div>
        {reviewers.isPending ? <Skeleton className="h-12 rounded-lg" />
          : reviewers.isError ? <div className="flex items-center justify-between gap-2 text-xs text-destructive" role="alert"><span>{t('profile.externalAccounts.pluginReview.listError')}</span><Button size="sm" variant="light" onPress={() => void reviewers.refetch()}>{t('profile.actions.retry')}</Button></div>
            : plugins.length === 0 ? <p className="rounded-lg bg-default-50 px-3 py-4 text-center text-xs text-muted-foreground">{t('profile.externalAccounts.pluginReview.none')}</p>
              : <>
                <div className="grid gap-1.5">
                  <label htmlFor="external-account-snapshot-plugin" className="text-xs font-medium">{t('profile.externalAccounts.pluginReview.plugin')}</label>
                  <Select id="external-account-snapshot-plugin" aria-label={t('profile.externalAccounts.pluginReview.plugin')} size="sm" selectedKeys={selectedPluginId ? [selectedPluginId] : []} isDisabled={review.isPending} onSelectionChange={(keys) => { if (keys === 'all') return; const [key] = Array.from(keys); if (typeof key === 'string') { setSelection(key); setAllowExternalReads(false); review.reset() } }}>
                    {plugins.map((plugin) => <SelectItem key={plugin.pluginId} textValue={`${plugin.displayName} ${plugin.pluginVersion}`}>{plugin.displayName} · {plugin.pluginVersion}</SelectItem>)}
                  </Select>
                </div>
                <Checkbox isSelected={allowExternalReads} isDisabled={review.isPending || !selectedPlugin?.supportsExternalReads} onValueChange={setAllowExternalReads}>
                  <span className="text-xs leading-5">{t('profile.externalAccounts.pluginReview.allowExternalReads')}</span>
                </Checkbox>
                <div className="flex justify-end"><Button color="primary" size="sm" isDisabled={!selectedPluginId || review.isPending} isLoading={review.isPending} onPress={() => void runReview()}>{t('profile.externalAccounts.pluginReview.run')}</Button></div>
                {review.isError ? <div className="flex items-center justify-between gap-2 text-xs text-destructive" role="alert"><span>{t('profile.externalAccounts.pluginReview.runError')}</span><Button size="sm" variant="light" onPress={() => void runReview()}>{t('profile.actions.retry')}</Button></div> : null}
                {review.data ? <section className="grid gap-3" aria-live="polite">
                  <div className="rounded-lg border border-[var(--hairline)] p-3">
                    <div className="flex flex-wrap items-center justify-between gap-2 text-[11px] text-muted-foreground">
                      <span>{review.data.displayName} · {review.data.pluginId} · {review.data.pluginVersion}</span>
                      <span>{t('profile.externalAccounts.pluginReview.snapshotRange', {
                        count: review.data.snapshotCount,
                        from: observedAt(review.data.firstObservedAt, locale),
                        to: observedAt(review.data.lastObservedAt, locale),
                      })}</span>
                    </div>
                    <p className="mt-2 whitespace-pre-wrap break-words text-sm leading-6">{review.data.summary}</p>
                  </div>
                  {review.data.checks.length === 0 ? <p className="text-xs text-muted-foreground">{t('profile.externalAccounts.pluginReview.noChecks')}</p>
                    : <ul className="grid gap-2">{review.data.checks.map((check, index) => <li key={`${check.code}-${index}`} className={`grid gap-1 rounded-lg border p-3 ${checkStyle(check.status)}`}>
                      <div className="flex flex-wrap items-center gap-2 text-[11px]"><Chip size="sm" variant="flat">{t(`profile.externalAccounts.pluginReview.status.${check.status}`, { defaultValue: check.status })}</Chip><code className="break-all text-muted-foreground">{check.code}</code></div>
                      <h3 className="text-xs font-semibold">{check.title}</h3>
                      <p className="whitespace-pre-wrap break-words text-xs leading-5">{check.message}</p>
                      {check.recommendation ? <p className="whitespace-pre-wrap break-words text-xs leading-5 text-muted-foreground">{t('profile.externalAccounts.pluginReview.recommendation')}: {check.recommendation}</p> : null}
                    </li>)}</ul>}
                  <PluginUiViewsRenderer manifest={{ uiViews: review.data.uiViews }} slot="external_account" locale={i18n.resolvedLanguage ?? 'en'} />
                </section> : null}
              </>}
      </DrawerBody>
      <DrawerFooter><Button size="sm" variant="flat" isDisabled={review.isPending} onPress={onClose}>{t('profile.externalAccounts.pluginReview.close')}</Button></DrawerFooter>
    </DrawerContent>
  </Drawer>
}
