export type CursorPageItem = number | 'ellipsis'

/** 为已取得游标的页面生成紧凑页码，避免把未知远端页伪装成可跳转页面。 */
export function buildCursorPageItems(currentPage: number, visitedPageCount: number): CursorPageItem[] {
  if (visitedPageCount <= 0) return []
  if (visitedPageCount <= 7) {
    return Array.from({ length: visitedPageCount }, (_, index) => index + 1)
  }

  const safeCurrentPage = Math.min(Math.max(currentPage, 1), visitedPageCount)
  if (safeCurrentPage <= 4) return [1, 2, 3, 4, 5, 'ellipsis', visitedPageCount]
  if (safeCurrentPage >= visitedPageCount - 3) {
    return [1, 'ellipsis', ...Array.from({ length: 5 }, (_, index) => visitedPageCount - 4 + index)]
  }
  return [1, 'ellipsis', safeCurrentPage - 1, safeCurrentPage, safeCurrentPage + 1, 'ellipsis', visitedPageCount]
}
