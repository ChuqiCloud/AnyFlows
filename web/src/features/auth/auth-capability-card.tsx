import type { ReactNode } from 'react'

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'

type AuthCapabilityCardProps = {
  action: ReactNode
  body: string
  icon: ReactNode
  title: string
}

/** 登录与注册能力不可用时使用的统一稳定状态卡。 */
export function AuthCapabilityCard({ action, body, icon, title }: AuthCapabilityCardProps) {
  return (
    <Card elevation="overlay" className="mx-auto w-full max-w-[430px] bg-card/94 backdrop-blur-2xl">
      <CardHeader className="gap-2 px-7 pt-7 pb-5">
        <div className="mb-2 grid size-10 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-2 text-muted-foreground">
          {icon}
        </div>
        <CardTitle className="text-xl leading-tight">{title}</CardTitle>
        <CardDescription className="leading-6">{body}</CardDescription>
      </CardHeader>
      <CardContent className="px-7 pb-7">{action}</CardContent>
    </Card>
  )
}

/** 用最终表单形状占位，避免公开能力读取期间发生布局跳动。 */
export function AuthCapabilitySkeleton({ label }: { label: string }) {
  return (
    <Card elevation="overlay" className="mx-auto w-full max-w-[430px] bg-card/94 backdrop-blur-2xl" aria-label={label}>
      <CardHeader className="gap-3 px-7 pt-7">
        <Skeleton className="size-10 rounded-xl" />
        <Skeleton className="h-6 w-40" />
        <Skeleton className="h-4 w-64 max-w-full" />
      </CardHeader>
      <CardContent className="grid gap-4 px-7 pb-7">
        {[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-11 rounded-lg" />)}
        <Skeleton className="h-11 rounded-lg" />
      </CardContent>
    </Card>
  )
}
