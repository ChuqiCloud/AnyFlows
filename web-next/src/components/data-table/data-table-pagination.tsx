import { Button, Select, SelectItem } from '@heroui/react'
import { ChevronLeft, ChevronRight, ChevronsLeft } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { buildPaginationItems } from './data-table-pagination-model'

export const DEFAULT_TABLE_PAGE_SIZE = 30
export const TABLE_PAGE_SIZE_OPTIONS = [20, 30, 50, 100] as const

/** HeroUI Select 的动态选项必须走 items + 渲染函数。 */
const PAGE_SIZE_ITEMS = TABLE_PAGE_SIZE_OPTIONS.map((value) => ({ key: String(value), label: String(value) }))

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
          <Button isIconOnly aria-label={t('tablePagination.actions.first')} isDisabled={currentPage === 1 || fetching} size="sm" type="button" variant="light" onClick={onFirstPage}>
            <ChevronsLeft className="size-3.5" aria-hidden="true" />
          </Button>
          <Button isIconOnly aria-label={t('tablePagination.actions.previous')} isDisabled={currentPage === 1 || fetching} size="sm" type="button" variant="light" onClick={onPreviousPage}>
            <ChevronLeft className="size-3.5" aria-hidden="true" />
          </Button>
          {pageItems.map((item, index) => item === 'ellipsis'
            ? <span key={`ellipsis-${index}`} className="grid size-8 place-items-center" aria-hidden="true">…</span>
            : (
              <Button
                isIconOnly
                key={item}
                aria-current={item === currentPage ? 'page' : undefined}
                aria-label={t('tablePagination.pageNumber', { page: item })}
                isDisabled={fetching}
                size="sm"
                type="button"
                variant={item === currentPage ? 'bordered' : 'light'}
                onClick={() => onPageSelect(item)}
              >
                {item}
              </Button>
            ))}
          <Button isIconOnly aria-label={t('tablePagination.actions.next')} isDisabled={!hasNextPage || fetching} size="sm" type="button" variant="light" onClick={onNextPage}>
            <ChevronRight className="size-3.5" aria-hidden="true" />
          </Button>
        </nav>

        <label className="flex items-center gap-2 whitespace-nowrap">
          <span>{t('tablePagination.pageSize')}</span>
          <Select
            aria-label={t('tablePagination.pageSize')}
            className="w-20"
            items={PAGE_SIZE_ITEMS}
            selectedKeys={[String(pageSize)]}
            size="sm"
            onSelectionChange={(keys) => onPageSizeChange(Number(Array.from(keys)[0]))}
          >
            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
          </Select>
        </label>
      </div>
    </footer>
  )
}
