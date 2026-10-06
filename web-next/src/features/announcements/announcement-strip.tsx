import { Megaphone } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { usePublicAnnouncements } from './announcement-api'

/** 公开首页公告条带只消费服务端已经过滤后的已发布事实。 */
export function AnnouncementStrip() {
  const { t, i18n } = useTranslation()
  const query = usePublicAnnouncements()
  const announcements = query.data?.entries ?? []
  if (announcements.length === 0) return null
  const language = i18n.language.toLowerCase().startsWith('zh') ? 'zh' : 'en'

  return (
    <section className="border-b border-[var(--hairline)] bg-surface-2/35" aria-label={t('announcements.public.label')}>
      <div className="mx-auto flex max-w-[1120px] gap-4 overflow-x-auto px-6 py-3 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
        {announcements.map((announcement) => (
          <article key={announcement.id} className="flex min-w-[min(30rem,88vw)] items-start gap-3 border-r border-[var(--hairline)] pr-6 last:border-r-0">
            <Megaphone className="mt-0.5 size-4 shrink-0 text-brand" aria-hidden="true" />
            <div className="min-w-0">
              <h2 className="truncate text-sm font-semibold">
                {language === 'zh' ? announcement.title_zh : announcement.title_en}
              </h2>
              <p className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">
                {language === 'zh' ? announcement.body_zh : announcement.body_en}
              </p>
            </div>
          </article>
        ))}
      </div>
    </section>
  )
}
