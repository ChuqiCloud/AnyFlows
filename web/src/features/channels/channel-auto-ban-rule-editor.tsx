import type { KeyboardEvent } from 'react'
import { useMemo, useState } from 'react'
import { Plus, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'

type ChannelAutoBanRuleEditorProps = {
  statusCodes: number[]
  keywords: string[]
  invalid?: boolean
  onStatusCodesChange: (statusCodes: number[]) => void
  onKeywordsChange: (keywords: string[]) => void
}

const serverStatusCodes = Array.from({ length: 100 }, (_, index) => index + 500)

/** 使用选择器和徽标维护有界自动停用规则，避免管理员手写 JSON。 */
export function ChannelAutoBanRuleEditor({
  statusCodes,
  keywords,
  invalid,
  onStatusCodesChange,
  onKeywordsChange,
}: ChannelAutoBanRuleEditorProps) {
  const { t } = useTranslation()
  const [selectedStatus, setSelectedStatus] = useState(503)
  const [keywordDraft, setKeywordDraft] = useState('')
  const ruleCount = statusCodes.length + keywords.length
  const selectedStatusExists = statusCodes.includes(selectedStatus)
  const availableStatuses = useMemo(
    () => serverStatusCodes.map((status) => ({ status, selected: statusCodes.includes(status) })),
    [statusCodes],
  )

  const addStatus = () => {
    if (selectedStatusExists || ruleCount >= 64) return
    onStatusCodesChange([...statusCodes, selectedStatus].sort((left, right) => left - right))
  }
  const addKeyword = () => {
    const keyword = keywordDraft.trim()
    if (!keyword || ruleCount >= 64) return
    const exists = keywords.some((item) => item.toLowerCase() === keyword.toLowerCase())
    if (!exists) onKeywordsChange([...keywords, keyword])
    setKeywordDraft('')
  }
  const handleKeywordKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault()
      addKeyword()
    }
  }

  return (
    <div className="grid gap-4 rounded-lg border border-input bg-[var(--surface-sunken)] p-3" aria-invalid={invalid}>
      <div className="grid gap-2">
        <div className="flex items-center justify-between gap-3">
          <Label htmlFor="channel-auto-ban-status">{t('channels.fields.autoBanStatusCodes')}</Label>
          <span className="font-mono text-[0.6875rem] text-muted-foreground">{ruleCount}/64</span>
        </div>
        <div className="grid grid-cols-[minmax(0,1fr)_2rem] gap-2">
          <Select
            id="channel-auto-ban-status"
            value={selectedStatus}
            disabled={ruleCount >= 64}
            onChange={(event) => setSelectedStatus(Number(event.target.value))}
          >
            {availableStatuses.map(({ status, selected }) => (
              <option key={status} value={status} disabled={selected}>HTTP {status}</option>
            ))}
          </Select>
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            disabled={selectedStatusExists || ruleCount >= 64}
            title={t('channels.actions.addAutoBanStatus')}
            aria-label={t('channels.actions.addAutoBanStatus')}
            onClick={addStatus}
          >
            <Plus aria-hidden="true" />
          </Button>
        </div>
        {statusCodes.length > 0 ? (
          <div className="flex flex-wrap gap-1.5">
            {statusCodes.map((status) => (
              <Badge key={status} className="h-7 gap-1 border-transparent bg-warning/10 pl-2 font-mono text-warning">
                HTTP {status}
                <button
                  type="button"
                  className="grid size-5 place-items-center rounded-sm hover:bg-warning/15 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  title={t('channels.actions.removeAutoBanStatus', { status })}
                  aria-label={t('channels.actions.removeAutoBanStatus', { status })}
                  onClick={() => onStatusCodesChange(statusCodes.filter((item) => item !== status))}
                >
                  <X className="size-3" aria-hidden="true" />
                </button>
              </Badge>
            ))}
          </div>
        ) : null}
      </div>

      <div className="grid gap-2">
        <Label htmlFor="channel-auto-ban-keyword">{t('channels.fields.autoBanKeywords')}</Label>
        <div className="grid grid-cols-[minmax(0,1fr)_2rem] gap-2">
          <Input
            id="channel-auto-ban-keyword"
            value={keywordDraft}
            maxLength={256}
            disabled={ruleCount >= 64}
            placeholder={t('channels.form.autoBanKeywordPlaceholder')}
            onBlur={addKeyword}
            onChange={(event) => setKeywordDraft(event.target.value)}
            onKeyDown={handleKeywordKeyDown}
          />
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            disabled={!keywordDraft.trim() || ruleCount >= 64}
            title={t('channels.actions.addAutoBanKeyword')}
            aria-label={t('channels.actions.addAutoBanKeyword')}
            onMouseDown={(event) => event.preventDefault()}
            onClick={addKeyword}
          >
            <Plus aria-hidden="true" />
          </Button>
        </div>
        {keywords.length > 0 ? (
          <div className="flex flex-wrap gap-1.5">
            {keywords.map((keyword) => (
              <Badge key={keyword} className="h-7 max-w-full gap-1 border-transparent bg-info/10 pl-2 text-info">
                <span className="truncate font-mono">{keyword}</span>
                <button
                  type="button"
                  className="grid size-5 shrink-0 place-items-center rounded-sm hover:bg-info/15 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  title={t('channels.actions.removeAutoBanKeyword', { keyword })}
                  aria-label={t('channels.actions.removeAutoBanKeyword', { keyword })}
                  onClick={() => onKeywordsChange(keywords.filter((item) => item !== keyword))}
                >
                  <X className="size-3" aria-hidden="true" />
                </button>
              </Badge>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  )
}
