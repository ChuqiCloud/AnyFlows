import { useEffect, useRef, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { ImageOff, LayoutDashboard } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { apiClient } from '@/lib/api'
import type { FrontendTemplateSummary } from './frontend-template-api'

/** 图片通过会话鉴权读取；Blob URL 在组件卸载或替换时释放。 */
export function FrontendTemplatePreview({ template }: {
  template: FrontendTemplateSummary
}) {
  const { t } = useTranslation()
  const container = useRef<HTMLDivElement>(null)
  const image = useRef<HTMLImageElement>(null)
  const [visible, setVisible] = useState(() => typeof IntersectionObserver === 'undefined')
  const preview = useQuery({
    queryKey: ['frontend-template-preview', template.id, template.version, template.preview_url],
    enabled: visible && Boolean(template.preview_url),
    staleTime: Infinity,
    gcTime: 5 * 60_000,
    retry: false,
    queryFn: async ({ signal }) => {
      const { data } = await apiClient.get<{ 200: Blob }>({
        url: '/api/admin/frontend-templates/{template_id}/preview',
        path: { template_id: template.id },
        security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
        parseAs: 'blob',
        headers: { Accept: 'image/png, image/jpeg, image/webp, image/svg+xml' },
        signal,
      })
      return data
    },
  })

  useEffect(() => {
    if (!container.current || typeof IntersectionObserver === 'undefined') return
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) { setVisible(true); observer.disconnect() }
    }, { rootMargin: '120px' })
    observer.observe(container.current)
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    if (!preview.data || !image.current) return
    const url = URL.createObjectURL(preview.data)
    image.current.src = url
    return () => URL.revokeObjectURL(url)
  }, [preview.data])

  if (preview.data) {
    return <div ref={container} className="h-full w-full"><img ref={image} loading="lazy" decoding="async" alt={t('siteSettings.templates.previewAlt', { name: template.name })} className="h-full w-full object-contain" /></div>
  }
  return (
    <div ref={container} className="flex h-full min-h-32 flex-col items-center justify-center gap-2 bg-surface-2/50 p-4 text-center text-muted-foreground">
      {preview.isError ? <ImageOff className="size-8" aria-hidden="true" /> : <LayoutDashboard className="size-8" aria-hidden="true" />}
      <span className="text-xs">{t(preview.isError ? 'siteSettings.templates.previewError' : template.preview_url ? 'siteSettings.templates.previewLoading' : 'siteSettings.templates.noPreview')}</span>
    </div>
  )
}
