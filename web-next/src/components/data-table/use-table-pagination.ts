import { useCallback, useEffect, useState } from 'react'

import { DEFAULT_TABLE_PAGE_SIZE } from './data-table-pagination'
import { getPageSlice } from './data-table-pagination-model'

type Cursor = number | string

/** 管理服务端游标历史，使用户只能跳转到已经取得游标的页面。 */
export function useCursorPagination<TCursor extends Cursor>(initialPageSize = DEFAULT_TABLE_PAGE_SIZE) {
  const [pageCursors, setPageCursors] = useState<Array<TCursor | undefined>>([undefined])
  const [pageIndex, setPageIndex] = useState(0)
  const [pageSize, setPageSizeState] = useState(initialPageSize)

  const reset = useCallback(() => {
    setPageCursors([undefined])
    setPageIndex(0)
  }, [])

  const setPageSize = useCallback((value: number) => {
    setPageSizeState(value)
    reset()
  }, [reset])

  const goToNextPage = useCallback((nextCursor: TCursor | null | undefined) => {
    if (pageIndex + 1 < pageCursors.length) {
      setPageIndex((current) => current + 1)
      return
    }
    if (nextCursor == null) return
    setPageCursors((current) => [...current, nextCursor])
    setPageIndex((current) => current + 1)
  }, [pageCursors.length, pageIndex])

  return {
    availablePageCount: pageCursors.length,
    cursor: pageCursors[pageIndex],
    currentPage: pageIndex + 1,
    goToFirstPage: () => setPageIndex(0),
    goToNextPage,
    goToPreviousPage: () => setPageIndex((current) => Math.max(0, current - 1)),
    hasLoadedNextPage: pageIndex + 1 < pageCursors.length,
    pageIndex,
    pageSize,
    reset,
    selectPage: (page: number) => setPageIndex(Math.min(Math.max(page - 1, 0), pageCursors.length - 1)),
    setPageSize,
  }
}

/** 管理已由 TanStack Query 取得的游标页，并仅在加载成功后进入新页。 */
export function useLoadedCursorPagination({
  availablePageCount,
  pageSize,
  onPageSizeChange,
}: {
  availablePageCount: number
  pageSize: number
  onPageSizeChange: (value: number) => void
}) {
  const safePageCount = Math.max(1, availablePageCount)
  const [pageIndex, setPageIndex] = useState(0)
  const safePageIndex = Math.min(pageIndex, safePageCount - 1)

  useEffect(() => {
    setPageIndex((current) => Math.min(current, safePageCount - 1))
  }, [safePageCount])

  const goToNextPage = async (hasRemoteNextPage: boolean, fetchNextPage: () => Promise<boolean>) => {
    if (safePageIndex + 1 < safePageCount) {
      setPageIndex((current) => current + 1)
      return
    }
    if (hasRemoteNextPage && await fetchNextPage()) {
      setPageIndex((current) => current + 1)
    }
  }

  return {
    availablePageCount: safePageCount,
    currentPage: safePageIndex + 1,
    goToFirstPage: () => setPageIndex(0),
    goToNextPage,
    goToPreviousPage: () => setPageIndex((current) => Math.max(0, current - 1)),
    hasLoadedNextPage: safePageIndex + 1 < safePageCount,
    pageIndex: safePageIndex,
    pageSize,
    reset: () => setPageIndex(0),
    selectPage: (page: number) => setPageIndex(Math.min(Math.max(page - 1, 0), safePageCount - 1)),
    setPageSize: (value: number) => {
      setPageIndex(0)
      onPageSizeChange(value)
    },
  }
}

/** 为已完整加载的小型目录提供准确总页数和切片边界。 */
export function useClientTablePagination(itemCount: number, initialPageSize = DEFAULT_TABLE_PAGE_SIZE) {
  const [pageIndex, setPageIndex] = useState(0)
  const [pageSize, setPageSize] = useState(initialPageSize)
  const page = getPageSlice(itemCount, pageSize, pageIndex)

  useEffect(() => {
    setPageIndex((current) => Math.min(current, page.pageCount - 1))
  }, [page.pageCount])

  return {
    availablePageCount: page.pageCount,
    currentPage: page.pageIndex + 1,
    endIndex: page.endIndex,
    goToFirstPage: () => setPageIndex(0),
    goToNextPage: () => setPageIndex((current) => Math.min(current + 1, page.pageCount - 1)),
    goToPreviousPage: () => setPageIndex((current) => Math.max(0, current - 1)),
    hasNextPage: page.pageIndex + 1 < page.pageCount,
    pageSize,
    selectPage: (value: number) => setPageIndex(Math.min(Math.max(value - 1, 0), page.pageCount - 1)),
    setPageSize: (value: number) => {
      setPageIndex(0)
      setPageSize(value)
    },
    startIndex: page.startIndex,
  }
}
