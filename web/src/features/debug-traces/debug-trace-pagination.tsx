import { ChevronLeft, ChevronRight, ChevronsLeft } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import { buildCursorPageItems } from './debug-trace-pagination-model'

const pageSizeOptions = [20, 30, 50, 100] as const

type DebugTracePaginationProps = {
  currentPage: number
  visitedPageCount: number
  pageSize: number
  itemCount: number
  hasNextPage: boolean
  fetching: boolean
  onFirstPage: () => void
  onPreviousPage: () => void
  onPageSelect: (page: number) => void
  onNextPage: () => void
  onPageSizeChange: (pageSize: number) => void
}

/** 游标分页只开放已访问页跳转，并明确展示当前页是否仍有后续记录。 */
export function DebugTracePagination({
  currentPage,
  visitedPageCount,
  pageSize,
  itemCount,
  hasNextPage,
  fetching,
  onFirstPage,
  onPreviousPage,
  onPageSelect,
  onNextPage,
  onPageSizeChange,
}: DebugTracePaginationProps) {
  const { t } = useTranslation()
  const pageItems = buildCursorPageItems(currentPage, visitedPageCount)

  return (
    <footer className="flex min-h-10 flex-col gap-3 text-xs text-muted-foreground sm:flex-row sm:items-center sm:justify-between">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span>{t('debugTraces.pageSummary', { page: currentPage, count: itemCount })}</span>
        <span aria-hidden="true">·</span>
        <span className={hasNextPage ? 'text-info' : undefined}>
          {t(hasNextPage ? 'debugTraces.moreAvailable' : 'debugTraces.endReached')}
        </span>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <nav className="flex items-center gap-1" aria-label={t('debugTraces.pagination')}>
          <Button type="button" size="icon-sm" variant="ghost" aria-label={t('debugTraces.actions.first')} disabled={currentPage === 1 || fetching} onClick={onFirstPage}>
            <ChevronsLeft aria-hidden="true" />
          </Button>
          <Button type="button" size="icon-sm" variant="ghost" aria-label={t('debugTraces.actions.previous')} disabled={currentPage === 1 || fetching} onClick={onPreviousPage}>
            <ChevronLeft aria-hidden="true" />
          </Button>
          {pageItems.map((item, index) => item === 'ellipsis'
            ? <span key={`ellipsis-${index}`} className="grid size-8 place-items-center" aria-hidden="true">…</span>
            : (
              <Button
                key={item}
                type="button"
                size="icon-sm"
                variant={item === currentPage ? 'secondary' : 'ghost'}
                aria-current={item === currentPage ? 'page' : undefined}
                aria-label={t('debugTraces.pageNumber', { page: item })}
                disabled={fetching}
                onClick={() => onPageSelect(item)}
              >
                {item}
              </Button>
            ))}
          <Button type="button" size="icon-sm" variant="ghost" aria-label={t('debugTraces.actions.next')} disabled={!hasNextPage || fetching} onClick={onNextPage}>
            <ChevronRight aria-hidden="true" />
          </Button>
        </nav>

        <label className="flex items-center gap-2 whitespace-nowrap">
          <span>{t('debugTraces.pageSize')}</span>
          <Select className="h-8 w-20 text-xs" value={String(pageSize)} onChange={(event) => onPageSizeChange(Number(event.target.value))}>
            {pageSizeOptions.map((value) => <option key={value} value={value}>{value}</option>)}
          </Select>
        </label>
      </div>
    </footer>
  )
}
