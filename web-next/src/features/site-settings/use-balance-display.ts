import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import {
  DEFAULT_QUOTA_DISPLAY_POLICY,
  formatQuota,
  type QuotaDisplayPolicy,
} from '@/lib/quota-display'
import { usePublicSiteSettings } from './site-settings-api'

/** 读取公开展示策略并提供全站一致的余额格式化入口。 */
export function useBalanceDisplay() {
  const { i18n } = useTranslation()
  const siteQuery = usePublicSiteSettings()
  const policy: QuotaDisplayPolicy = siteQuery.data?.balance_display ?? DEFAULT_QUOTA_DISPLAY_POLICY
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'

  return useMemo(() => ({
    formatQuota: (value: bigint | number | string) => formatQuota(value, policy, locale),
    mode: policy.mode,
    policy,
    unitName: policy.mode === 'quota' ? undefined : policy.unit_name,
  }), [locale, policy])
}
