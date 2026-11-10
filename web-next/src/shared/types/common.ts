/**
 * 共享 TypeScript 类型定义
 *
 * 这些类型可在整个应用中复用，避免重复定义。
 */

/**
 * 基础编辑器 Props
 *
 * 适用于所有编辑器抽屉组件的通用 Props
 *
 * @template T - 被编辑的数据类型
 * @template CreateResult - 创建操作的返回类型（默认与 T 相同）
 * @template UpdateResult - 更新操作的返回类型（默认与 T 相同）
 */
export interface BaseEditorProps<T, CreateResult = T, UpdateResult = T> {
  /** 抽屉是否打开 */
  open: boolean
  /** 被编辑的项目（创建时为 undefined） */
  item?: T
  /** 抽屉开关状态改变回调 */
  onOpenChange: (open: boolean) => void
  /** 创建成功回调 */
  onCreate?: (result: CreateResult) => void
  /** 更新成功回调 */
  onUpdate?: (id: number, result: UpdateResult) => void
  /** 删除成功回调 */
  onDelete?: (id: number) => void
}

/**
 * 分页参数
 */
export interface PaginationParams {
  /** 当前页码（从 1 开始） */
  page: number
  /** 每页数量 */
  pageSize: number
}

/**
 * 分页响应
 */
export interface PaginatedResponse<T> {
  /** 数据列表 */
  items: T[]
  /** 总数量 */
  total: number
  /** 当前页码 */
  page: number
  /** 每页数量 */
  pageSize: number
  /** 总页数 */
  totalPages: number
}

/**
 * API 请求状态
 */
export type RequestStatus = 'idle' | 'loading' | 'success' | 'error'

/**
 * 通用 API 响应
 */
export interface ApiResponse<T = unknown> {
  success: boolean
  data?: T
  message?: string
  error?: string
}

/**
 * 表单字段错误
 */
export type FieldErrors<T> = {
  [K in keyof T]?: {
    type: string
    message: string
  }
}

/**
 * 确认对话框 Props
 */
export interface ConfirmDialogProps {
  open: boolean
  title: string
  description?: string
  confirmText?: string
  cancelText?: string
  variant?: 'default' | 'danger'
  loading?: boolean
  onConfirm: () => void
  onCancel: () => void
}

/**
 * 表格列定义
 */
export interface TableColumn<T> {
  /** 列键 */
  key: keyof T | string
  /** 列标题 */
  label: string
  /** 是否可排序 */
  sortable?: boolean
  /** 自定义渲染函数 */
  render?: (item: T) => React.ReactNode
  /** 列宽 */
  width?: string
  /** 对齐方式 */
  align?: 'left' | 'center' | 'right'
}

/**
 * 排序参数
 */
export interface SortParams<T> {
  /** 排序字段 */
  field: keyof T
  /** 排序方向 */
  direction: 'asc' | 'desc'
}

/**
 * 筛选参数
 */
export interface FilterParams {
  [key: string]: string | number | boolean | string[] | undefined
}
