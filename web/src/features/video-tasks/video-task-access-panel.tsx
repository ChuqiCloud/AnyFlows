import { Eye, EyeOff, KeyRound, Save, Trash2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

type VideoTaskAccessPanelProps = {
  apiKeyDraft: string
  apiKeyReady: boolean
  error?: string
  onChange: (value: string) => void
  onClear: () => void
  onSave: () => void
}

/** 复用 Playground 当前标签页 API Key 的结构化访问设置。 */
export function VideoTaskAccessPanel(props: VideoTaskAccessPanelProps) {
  const { t } = useTranslation()
  const [visible, setVisible] = useState(false)

  const save = (event: FormEvent) => {
    event.preventDefault()
    props.onSave()
  }

  return (
    <section className="grid gap-3 p-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-xs font-semibold">{t('videoTasks.access.title')}</h3>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">
            {t('videoTasks.access.description')}
          </p>
        </div>
        <Badge className={props.apiKeyReady ? 'bg-success/12 text-success' : 'bg-warning/12 text-warning'}>
          {t(props.apiKeyReady ? 'videoTasks.apiKey.ready' : 'videoTasks.apiKey.missing')}
        </Badge>
      </div>

      <form className="grid gap-2" onSubmit={save}>
        <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground" htmlFor="video-task-api-key">
          <span>{t('videoTasks.apiKey.label')}</span>
          <span className="relative block">
            <KeyRound className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2" aria-hidden="true" />
            <Input
              id="video-task-api-key"
              type={visible ? 'text' : 'password'}
              className="px-8 font-mono text-xs"
              value={props.apiKeyDraft}
              autoComplete="off"
              autoCapitalize="none"
              maxLength={512}
              spellCheck={false}
              aria-invalid={props.error !== undefined}
              placeholder="sk-af-..."
              onChange={(event) => props.onChange(event.target.value)}
            />
            <button
              type="button"
              className="absolute right-1 top-1/2 grid size-6 -translate-y-1/2 place-items-center rounded-md text-muted-foreground hover:bg-surface-2 hover:text-foreground"
              aria-label={t(visible ? 'videoTasks.apiKey.hide' : 'videoTasks.apiKey.show')}
              onClick={() => setVisible((current) => !current)}
            >
              {visible
                ? <EyeOff className="size-3.5" aria-hidden="true" />
                : <Eye className="size-3.5" aria-hidden="true" />}
            </button>
          </span>
        </label>
        {props.error ? <p role="alert" className="text-xs text-destructive">{props.error}</p> : null}
        <div className="grid grid-cols-2 gap-2">
          <Button type="submit" size="sm" variant="secondary">
            <Save aria-hidden="true" />{t('videoTasks.apiKey.save')}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={!props.apiKeyDraft && !props.apiKeyReady}
            onClick={props.onClear}
          >
            <Trash2 aria-hidden="true" />{t('videoTasks.apiKey.clear')}
          </Button>
        </div>
      </form>
    </section>
  )
}
