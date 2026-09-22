import type { ReactNode } from 'react'
import { Card, CardBody, CardHeader, Skeleton } from '@heroui/react'

import { cn } from '@/lib/utils'

/*
 * 卡片外观沿用原设计系统：HeroUI Card 自带 bg-content1 / h-auto，
 * 这里用 className 覆盖回 1px 细边界，twMerge 保证覆盖生效。
 * 圆角无需处理——HeroUI 的 rounded-large 同为 14px。
 */
const cardClass = 'border border-[var(--hairline)] bg-card text-card-foreground'

type AuthCapabilityCardProps = {
  action: ReactNode
  body: string
  icon: ReactNode
  title: string
}

/** 登录与注册能力不可用时使用的统一稳定状态卡。 */
export function AuthCapabilityCard({ action, body, icon, title }: AuthCapabilityCardProps) {
  return (
    <Card shadow="none" className={cn(cardClass, 'mx-auto w-full max-w-[430px] bg-card/94 backdrop-blur-2xl')}>
      {/* HeroUI 头部默认横排 + items-center + p-3，补回原竖排与 p-5 基准。 */}
      <CardHeader className="flex-col items-stretch gap-1.5 p-5 gap-2 px-7 pt-7 pb-5">
        <div className="mb-2 grid size-10 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-2 text-muted-foreground">
          {icon}
        </div>
        <h3 data-slot="card-title" className="text-[0.9375rem] leading-none font-semibold text-xl leading-tight">{title}</h3>
        <p data-slot="card-description" className="text-sm text-muted-foreground leading-6">{body}</p>
      </CardHeader>
      <CardBody className="p-5 pt-0 px-7 pb-7">{action}</CardBody>
    </Card>
  )
}

/** 用最终表单形状占位，避免公开能力读取期间发生布局跳动。 */
export function AuthCapabilitySkeleton({ label }: { label: string }) {
  return (
    <Card
      shadow="none"
      aria-label={label}
      className={cn(cardClass, 'mx-auto w-full max-w-[430px] bg-card/94 backdrop-blur-2xl')}
    >
      <CardHeader className="flex-col items-stretch gap-1.5 p-5 gap-3 px-7 pt-7">
        <Skeleton disableAnimation className="animate-pulse rounded-md bg-muted size-10 rounded-xl" />
        <Skeleton disableAnimation className="animate-pulse rounded-md bg-muted h-6 w-40" />
        <Skeleton disableAnimation className="animate-pulse rounded-md bg-muted h-4 w-64 max-w-full" />
      </CardHeader>
      <CardBody className="p-5 pt-0 grid gap-4 px-7 pb-7">
        {[0, 1, 2, 3].map((item) => <Skeleton key={item} disableAnimation className="animate-pulse rounded-md bg-muted h-11 rounded-lg" />)}
        <Skeleton disableAnimation className="animate-pulse rounded-md bg-muted h-11 rounded-lg" />
      </CardBody>
    </Card>
  )
}
