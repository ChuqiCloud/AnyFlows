import { useTranslation } from 'react-i18next'
import type { MerchantMaintenance } from './merchant-extension-model'
import { poolTimestamp } from './merchant-pool-model'
import { MerchantManagementTable } from './merchant-management-tables'

export function MerchantPublicMaintenance({ notices }: { notices: MerchantMaintenance[] }) {
  const { t, i18n } = useTranslation()
  if (notices.length === 0) return null
  return <MerchantManagementTable label={t('merchantPool.lifecycleTabs.maintenance')} columns={['description', 'starts', 'ends', 'blocking']}>{notices.map((notice) => <tr key={notice.id}>{[notice.message, poolTimestamp(notice.starts_at, i18n.language), poolTimestamp(notice.ends_at, i18n.language), t(notice.blocks_requests ? 'merchantPool.yes' : 'merchantPool.no')].map((value, index) => <td key={index} className={`border-b border-[var(--hairline)] px-2 py-2 align-middle ${index === 0 ? 'min-w-48 max-w-96 whitespace-normal' : 'whitespace-nowrap'}`}>{value}</td>)}</tr>)}</MerchantManagementTable>
}
