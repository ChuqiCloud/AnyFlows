import { useTranslation } from 'react-i18next'

import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
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
    <Drawer
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-2xl lg:max-w-3xl' }}
      isOpen={traceId !== undefined}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="sr-only">
              <h2>{t('debugTraces.detail.sheetTitle')}</h2>
              <p>{t('debugTraces.detail.sheetDescription')}</p>
            </DrawerHeader>
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-y-auto overscroll-contain p-0">
              {traceId !== undefined ? (
                <DebugTraceDetail
                  key={traceId}
                  traceId={traceId}
                  detail={query.data}
                  pending={query.isPending}
                  error={query.isError}
                  onRetry={() => void query.refetch()}
                />
              ) : null}
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
