import type { ReactNode } from 'react'
import {
  Drawer,
  DrawerBody,
  DrawerContent,
  DrawerHeader,
} from '@heroui/react'

import { cn } from '@/lib/utils'

export interface EditorSheetProps {
  open: boolean
  title: string
  description?: string
  maxWidth?: 'sm' | 'md' | 'lg' | 'xl' | '2xl'
  children: ReactNode
  onOpenChange: (open: boolean) => void
}

/**
 * 通用编辑器抽屉组件
 *
 * 用于替代项目中 10+ 个重复的 *-editor-sheet.tsx 组件。
 * 提供统一的样式、布局和交互行为。
 *
 * @example
 * ```tsx
 * <EditorSheet
 *   open={isOpen}
 *   title="创建 API Key"
 *   description="填写以下信息创建新的 API Key"
 *   maxWidth="xl"
 *   onOpenChange={setIsOpen}
 * >
 *   <YourFormComponent />
 * </EditorSheet>
 * ```
 */
export function EditorSheet({
  open,
  title,
  description,
  maxWidth = 'xl',
  children,
  onOpenChange,
}: EditorSheetProps) {
  const maxWidthClasses = {
    sm: 'sm:max-w-sm',
    md: 'sm:max-w-md',
    lg: 'sm:max-w-lg',
    xl: 'sm:max-w-xl',
    '2xl': 'sm:max-w-2xl',
  }

  return (
    <Drawer
      backdrop="blur"
      classNames={{
        base: cn('w-full max-h-none', maxWidthClasses[maxWidth]),
      }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
          <h2 className="text-base font-medium text-foreground">{title}</h2>
          {description && (
            <p className="text-sm text-muted-foreground">{description}</p>
          )}
        </DrawerHeader>
        <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
          {children}
        </DrawerBody>
      </DrawerContent>
    </Drawer>
  )
}
