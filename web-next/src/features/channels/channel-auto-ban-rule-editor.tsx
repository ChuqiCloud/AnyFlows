import type { KeyboardEvent } from 'react'
import { useMemo, useState } from 'react'
import { Button, Chip, Input, Select, SelectItem } from '@heroui/react'
import { Plus } from 'lucide-react'
import { useTranslation } from 'react-i18next'

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
  // HeroUI Select 的动态选项必须走 items + 渲染函数。
  const availableStatuses = useMemo(
    () => serverStatusCodes.map((status) => ({ key: String(status), label: `HTTP ${status}`, selected: statusCodes.includes(status) })),
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
          <label className="text-xs font-medium leading-none text-foreground" htmlFor="channel-auto-ban-status">{t('channels.fields.autoBanStatusCodes')}</label>
          <span className="font-mono text-[0.6875rem] text-muted-foreground">{ruleCount}/64</span>
        </div>
        <div className="grid grid-cols-[minmax(0,1fr)_2rem] gap-2">
          <Select
            aria-label={t('channels.fields.autoBanStatusCodes')}
            id="channel-auto-ban-status"
            isDisabled={ruleCount >= 64}
            items={availableStatuses}
            selectedKeys={[String(selectedStatus)]}
            size="sm"
            onSelectionChange={(keys) => setSelectedStatus(Number(Array.from(keys)[0]))}
          >
            {(item) => <SelectItem key={item.key} isDisabled={item.selected}>{item.label}</SelectItem>}
          </Select>
          <Button
            isIconOnly
            aria-label={t('channels.actions.addAutoBanStatus')}
            isDisabled={selectedStatusExists || ruleCount >= 64}
            size="sm"
            title={t('channels.actions.addAutoBanStatus')}
            type="button"
            variant="bordered"
            onClick={addStatus}
          >
            <Plus className="size-3.5" aria-hidden="true" />
          </Button>
        </div>
        {statusCodes.length > 0 ? (
          <div className="flex flex-wrap gap-1.5">
            {statusCodes.map((status) => (
              <Chip
                key={status}
                className="h-7 min-w-max shrink-0 !flex-nowrap whitespace-nowrap bg-warning/10 pl-2 font-mono text-warning"
                classNames={{
                  base: 'min-w-max shrink-0 !flex-nowrap whitespace-nowrap',
                  content: 'shrink-0 whitespace-nowrap',
                  closeButton: 'inline-flex size-5 shrink-0 self-center items-center justify-center [&>svg]:block',
                }}
                size="sm"
                variant="flat"
                onClose={() => onStatusCodesChange(statusCodes.filter((item) => item !== status))}
              >
                HTTP {status}
              </Chip>
            ))}
          </div>
        ) : null}
      </div>

      <div className="grid gap-2">
        <label className="text-xs font-medium leading-none text-foreground" htmlFor="channel-auto-ban-keyword">{t('channels.fields.autoBanKeywords')}</label>
        <div className="grid grid-cols-[minmax(0,1fr)_2rem] gap-2">
          <Input
            id="channel-auto-ban-keyword"
            isDisabled={ruleCount >= 64}
            maxLength={256}
            placeholder={t('channels.form.autoBanKeywordPlaceholder')}
            size="sm"
            value={keywordDraft}
            onBlur={addKeyword}
            onChange={(event) => setKeywordDraft(event.target.value)}
            onKeyDown={handleKeywordKeyDown}
          />
          <Button
            isIconOnly
            aria-label={t('channels.actions.addAutoBanKeyword')}
            isDisabled={!keywordDraft.trim() || ruleCount >= 64}
            size="sm"
            title={t('channels.actions.addAutoBanKeyword')}
            type="button"
            variant="bordered"
            onClick={addKeyword}
            onMouseDown={(event) => event.preventDefault()}
          >
            <Plus className="size-3.5" aria-hidden="true" />
          </Button>
        </div>
        {keywords.length > 0 ? (
          <div className="flex flex-wrap gap-1.5">
            {keywords.map((keyword) => (
              <Chip
                key={keyword}
                className="h-7 max-w-full shrink-0 !flex-nowrap whitespace-nowrap bg-info/10 pl-2 text-info"
                classNames={{
                  base: 'max-w-full shrink-0 !flex-nowrap whitespace-nowrap',
                  content: 'min-w-0 truncate whitespace-nowrap font-mono',
                  closeButton: 'inline-flex size-5 shrink-0 self-center items-center justify-center [&>svg]:block',
                }}
                size="sm"
                variant="flat"
                onClose={() => onKeywordsChange(keywords.filter((item) => item !== keyword))}
              >
                {keyword}
              </Chip>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  )
}
