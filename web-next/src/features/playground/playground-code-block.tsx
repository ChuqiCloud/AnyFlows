import { Button } from '@heroui/react'
import { Check, Copy } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

type PlaygroundCodeBlockProps = {
  code: string
  language?: string
  streaming: boolean
}

const maxHighlightedCodeLength = 50_000

/** 流结束后按需加载 Shiki；高亮失败时保留可复制的纯文本代码。 */
export function PlaygroundCodeBlock({ code, language, streaming }: PlaygroundCodeBlockProps) {
  const { t } = useTranslation()
  const [html, setHtml] = useState<string>()
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    setHtml(undefined)
    if (streaming || code.length > maxHighlightedCodeLength) return

    let active = true
    void import('shiki')
      .then(({ codeToHtml }) => codeToHtml(code, {
        lang: language || 'text',
        themes: { dark: 'github-dark-default', light: 'github-light-default' },
        defaultColor: false,
      }))
      .then((highlighted) => {
        if (active) setHtml(highlighted)
      })
      .catch(() => {
        if (active) setHtml(undefined)
      })

    return () => {
      active = false
    }
  }, [code, language, streaming])

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      setCopied(false)
    }
  }

  return (
    <div className="my-3 overflow-hidden rounded-xl border border-[var(--hairline)] bg-[var(--surface-sunken)]">
      <div className="flex h-8 items-center justify-between border-b border-[var(--hairline)] px-2.5 text-[0.6875rem] text-muted-foreground">
        <span className="min-w-0 truncate font-mono">{language || t('playground.code.plainText')}</span>
        <Button
          isIconOnly
          aria-label={t(copied ? 'playground.actions.copied' : 'playground.actions.copyCode')}
          className="size-6 min-w-6"
          size="sm"
          type="button"
          variant="light"
          onClick={copy}
        >
          {copied ? <Check className="size-3" aria-hidden="true" /> : <Copy className="size-3" aria-hidden="true" />}
        </Button>
      </div>
      {html ? (
        <div
          className="playground-shiki overflow-x-auto text-xs leading-5 [&_.shiki]:m-0 [&_.shiki]:min-w-max [&_.shiki]:bg-transparent! [&_.shiki]:p-3"
          // Shiki 只处理传入代码并生成受控 span，不执行模型返回的 HTML。
          dangerouslySetInnerHTML={{ __html: html }}
        />
      ) : (
        <pre className="overflow-x-auto p-3 text-xs leading-5"><code>{code}</code></pre>
      )}
    </div>
  )
}
