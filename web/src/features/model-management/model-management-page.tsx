import { Boxes, ScanSearch } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { ModelManagementCatalogWorkspace } from './model-management-catalog-workspace'
import { ModelSyncWorkspace } from './model-sync-workspace'

type ModelManagementView = 'catalog' | 'sync'

const views = [
  { id: 'catalog', Icon: Boxes },
  { id: 'sync', Icon: ScanSearch },
] as const

/** 在独立商品目录与受控上游同步之间切换，不混用两类服务端状态。 */
export function ModelManagementPage() {
  const { t } = useTranslation()
  const [view, setView] = useState<ModelManagementView>('catalog')

  return (
    <div className="flex flex-col gap-4">
      <header>
        <h2 className="text-lg font-semibold">{t('modelManagement.title')}</h2>
        <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('modelManagement.subtitle')}</p>
      </header>

      <div
        role="group"
        aria-label={t('modelManagement.views.label')}
        className="inline-flex w-fit max-w-full items-center gap-1 rounded-lg border border-[var(--hairline)] bg-surface-2/55 p-1"
      >
        {views.map(({ id, Icon }) => (
          <Button
            key={id}
            type="button"
            size="sm"
            variant="ghost"
            aria-pressed={view === id}
            className={cn('min-w-28', view === id && 'bg-surface-1 text-foreground shadow-sm hover:bg-surface-1')}
            onClick={() => setView(id)}
          >
            <Icon aria-hidden="true" />
            {t(`modelManagement.views.${id}`)}
          </Button>
        ))}
      </div>

      {view === 'catalog' ? <ModelManagementCatalogWorkspace /> : <ModelSyncWorkspace />}
    </div>
  )
}
