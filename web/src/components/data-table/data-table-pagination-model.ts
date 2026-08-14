export type PaginationItem = number | 'ellipsis'

/** 为可访问页面生成紧凑页码，不把未知的远端页面伪装成可跳转页。 */
export function buildPaginationItems(currentPage: number, availablePageCount: number): PaginationItem[] {
  if (availablePageCount <= 0) return []
  if (availablePageCount <= 7) {
    return Array.from({ length: availablePageCount }, (_, index) => index + 1)
  }

  const safeCurrentPage = Math.min(Math.max(currentPage, 1), availablePageCount)
  if (safeCurrentPage <= 4) return [1, 2, 3, 4, 5, 'ellipsis', availablePageCount]
  if (safeCurrentPage >= availablePageCount - 3) {
    return [1, 'ellipsis', ...Array.from({ length: 5 }, (_, index) => availablePageCount - 4 + index)]
  }
  return [1, 'ellipsis', safeCurrentPage - 1, safeCurrentPage, safeCurrentPage + 1, 'ellipsis', availablePageCount]
}

/** 将完整目录切成稳定页面，供无需再次请求服务端的小型管理表格使用。 */
export function getPageSlice(itemCount: number, pageSize: number, pageIndex: number) {
  const safePageSize = Math.max(1, pageSize)
  const pageCount = Math.max(1, Math.ceil(Math.max(0, itemCount) / safePageSize))
  const safePageIndex = Math.min(Math.max(0, pageIndex), pageCount - 1)
  return {
    endIndex: Math.min(itemCount, (safePageIndex + 1) * safePageSize),
    pageCount,
    pageIndex: safePageIndex,
    startIndex: safePageIndex * safePageSize,
  }
}
