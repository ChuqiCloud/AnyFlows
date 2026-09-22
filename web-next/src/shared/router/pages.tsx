import { useTranslation } from 'react-i18next'

type StatusPageProps = {
  code: string
  title: string
  description: string
}

function StatusPage({ code, title, description }: StatusPageProps) {
  return (
    <div className="flex min-h-[60vh] items-center justify-center py-10">
      <div className="w-full max-w-xl rounded-xl border border-[var(--hairline)] bg-surface-1 p-8 text-center">
        <p className="text-sm font-semibold tracking-[0.24em] text-brand tabular-nums">{code}</p>
        <h1 className="mt-4 text-2xl font-semibold tracking-tight text-foreground">{title}</h1>
        <p className="mt-3 text-sm leading-6 text-muted-foreground">{description}</p>
      </div>
    </div>
  )
}

/** 路由守卫在权限不足时渲染的占位页。 */
export function ForbiddenPage() {
  const { t } = useTranslation()

  return (
    <StatusPage
      code="403"
      description={t('shell.forbidden.description')}
      title={t('shell.forbidden.title')}
    />
  )
}

/** 地址无对应页面时渲染的占位页。 */
export function NotFoundPage() {
  const { t } = useTranslation()

  return (
    <StatusPage
      code="404"
      description={t('shell.notFound.description')}
      title={t('shell.notFound.title')}
    />
  )
}
