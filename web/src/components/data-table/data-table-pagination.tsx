import { ChevronLeft, ChevronRight, ChevronsLeft } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import { buildPaginationItems } from './data-table-pagination-model'

export const DEFAULT_TABLE_PAGE_SIZE = 30
export const TABLE_PAGE_SIZE_OPTIONS = [20, 30, 50, 100] as const

export type DataTablePaginationProps = {
  currentPage: number
  availablePageCount: number
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

/** 统一表格分页外观，并明确区分已知可访问页与未知远端页。 */
export function DataTablePagination({
  currentPage,
  availablePageCount,
  pageSize,
  itemCount,
  hasNextPage,
  fetching,
  onFirstPage,
  onPreviousPage,
  onPageSelect,
  onNextPage,
  onPageSizeChange,
}: DataTablePaginationProps) {
  const { t } = useTranslation()
  const pageItems = buildPaginationItems(currentPage, availablePageCount)

  return (
    <footer className="flex min-h-10 flex-col gap-3 text-xs text-muted-foreground sm:flex-row sm:items-center sm:justify-between">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span>{t('tablePagination.pageSummary', { page: currentPage, count: itemCount })}</span>
        <span aria-hidden="true">·</span>
        <span className={hasNextPage ? 'text-info' : undefined}>
          {t(hasNextPage ? 'tablePagination.moreAvailable' : 'tablePagination.endReached')}
        </span>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <nav className="flex items-center gap-1" aria-label={t('tablePagination.label')}>
          <Button type="button" size="icon-sm" variant="ghost" aria-label={t('tablePagination.actions.first')} disabled={currentPage === 1 || fetching} onClick={onFirstPage}>
            <ChevronsLeft aria-hidden="true" />
          </Button>
          <Button type="button" size="icon-sm" variant="ghost" aria-label={t('tablePagination.actions.previous')} disabled={currentPage === 1 || fetching} onClick={onPreviousPage}>
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
                aria-label={t('tablePagination.pageNumber', { page: item })}
                disabled={fetching}
                onClick={() => onPageSelect(item)}
              >
                {item}
              </Button>
            ))}
          <Button type="button" size="icon-sm" variant="ghost" aria-label={t('tablePagination.actions.next')} disabled={!hasNextPage || fetching} onClick={onNextPage}>
            <ChevronRight aria-hidden="true" />
          </Button>
        </nav>

        <label className="flex items-center gap-2 whitespace-nowrap">
          <span>{t('tablePagination.pageSize')}</span>
          <Select className="h-8 w-20 text-xs" value={String(pageSize)} onChange={(event) => onPageSizeChange(Number(event.target.value))}>
            {TABLE_PAGE_SIZE_OPTIONS.map((value) => <option key={value} value={value}>{value}</option>)}
          </Select>
        </label>
      </div>
    </footer>
  )
}
