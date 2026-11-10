import type { ReactNode } from 'react'

import { cn } from '@/lib/utils'

export interface FormFieldProps {
  id?: string
  label: string
  error?: string
  hint?: string
  required?: boolean
  disabled?: boolean
  children: ReactNode
  className?: string
}

/**
 * 通用表单字段组件
 *
 * 用于替代项目中 8+ 个重复的 *-form-field.tsx 组件。
 * 提供统一的标签、错误提示和帮助文本布局。
 *
 * @example
 * ```tsx
 * <FormField
 *   id="api-key-name"
 *   label="名称"
 *   hint="为这个 API Key 起一个易于识别的名称"
 *   error={errors.name?.message}
 *   required
 * >
 *   <Input {...register('name')} />
 * </FormField>
 * ```
 */
export function FormField({
  id,
  label,
  error,
  hint,
  required,
  disabled,
  children,
  className,
}: FormFieldProps) {
  return (
    <div className={cn('space-y-1.5', className)}>
      <label
        htmlFor={id}
        className={cn(
          'text-sm font-medium text-foreground',
          disabled && 'opacity-50',
        )}
      >
        {label}
        {required && <span className="ml-1 text-destructive">*</span>}
      </label>
      {children}
      {error && (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      )}
      {hint && !error && (
        <p className="text-sm text-muted-foreground">{hint}</p>
      )}
    </div>
  )
}
