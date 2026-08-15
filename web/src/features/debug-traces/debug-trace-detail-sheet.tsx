import { useTranslation } from 'react-i18next'

import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { useAdminDebugTrace } from './debug-trace-api'
import { DebugTraceDetail } from './debug-trace-detail'

type DebugTraceDetailSheetProps = {
  traceId?: number
  onOpenChange: (open: boolean) => void
}

/** 详情按需装入独立侧栏，关闭后不改变日志筛选与分页位置。 */
export function DebugTraceDetailSheet({ traceId, onOpenChange }: DebugTraceDetailSheetProps) {
  const { t } = useTranslation()
  const query = useAdminDebugTrace(traceId)

  return (
    <Sheet open={traceId !== undefined} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 overflow-hidden data-[side=right]:w-full data-[side=right]:sm:max-w-2xl data-[side=right]:lg:max-w-3xl">
        <SheetHeader className="sr-only">
          <SheetTitle>{t('debugTraces.detail.sheetTitle')}</SheetTitle>
          <SheetDescription>{t('debugTraces.detail.sheetDescription')}</SheetDescription>
        </SheetHeader>
        {traceId !== undefined ? (
          <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
            <DebugTraceDetail
              key={traceId}
              traceId={traceId}
              detail={query.data}
              pending={query.isPending}
              error={query.isError}
              onRetry={() => void query.refetch()}
            />
          </div>
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
