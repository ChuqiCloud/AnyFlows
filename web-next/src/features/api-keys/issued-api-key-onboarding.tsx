import { Button, Input } from '@heroui/react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Link } from 'react-router-dom'

import { copyText } from '@/lib/clipboard'
import { apiUrl } from '@/lib/api/endpoint'
import { EMPTY_MODEL_CATALOG_FILTERS, useModelCatalog } from '@/features/models/model-api'
import type { ModelCatalogItem, ModelCatalogProtocol } from '@/lib/api/generated/types.gen'

type SampleLanguage = 'curl' | 'python' | 'javascript'
type SupportedProtocol = Extract<ModelCatalogProtocol, 'openai_chat' | 'openai_responses' | 'anthropic'>

const supportedProtocols: SupportedProtocol[] = ['openai_chat', 'openai_responses', 'anthropic']

function modelProtocols(model?: ModelCatalogItem) {
  return supportedProtocols.filter((protocol) => model?.available_protocols.includes(protocol))
}

function makeSample(language: SampleLanguage, protocol: SupportedProtocol, model: string, endpoint: string) {
  const url = `${endpoint}${protocol === 'anthropic' ? '/messages' : protocol === 'openai_responses' ? '/responses' : '/chat/completions'}`
  const keyHeader = protocol === 'anthropic' ? '"x-api-key: $ANYFLOWS_API_KEY"' : '"Authorization: Bearer $ANYFLOWS_API_KEY"'
  const payload = protocol === 'anthropic'
    ? JSON.stringify({ model, max_tokens: 512, messages: [{ role: 'user', content: 'Hello' }] }, null, 2)
    : JSON.stringify(protocol === 'openai_responses'
      ? { model, input: 'Hello', max_output_tokens: 512 }
      : { model, messages: [{ role: 'user', content: 'Hello' }], max_tokens: 512 }, null, 2)

  if (language === 'curl') {
    const anthropicHeader = protocol === 'anthropic' ? "  -H 'anthropic-version: 2023-06-01' " + String.fromCharCode(92) + String.fromCharCode(10) : ''
    return `export ANYFLOWS_BASE_URL="${endpoint}"
export ANYFLOWS_API_KEY="<your-api-key>"

curl "${url}" \\
  -H "Content-Type: application/json" \\
  -H ${keyHeader} \\
${anthropicHeader}
  -d '${payload.replaceAll("'", "'\\''")}'`
  }

  if (language === 'python') {
    if (protocol === 'anthropic') {
      return `import os
import anthropic

client = anthropic.Anthropic(
    api_key=os.environ["ANYFLOWS_API_KEY"],
    base_url=os.environ["ANYFLOWS_BASE_URL"],
)
response = client.messages.create(
    model=${JSON.stringify(model)},
    max_tokens=512,
    messages=[{"role": "user", "content": "Hello"}],
)
print(response.content[0].text)`
    }
    if (protocol === 'openai_responses') {
      return `import os
from openai import OpenAI

client = OpenAI(
    api_key=os.environ["ANYFLOWS_API_KEY"],
    base_url=os.environ["ANYFLOWS_BASE_URL"],
)
response = client.responses.create(
    model=${JSON.stringify(model)},
    input="Hello",
)
print(response.output_text)`
    }
    return `import os
from openai import OpenAI

client = OpenAI(
    api_key=os.environ["ANYFLOWS_API_KEY"],
    base_url=os.environ["ANYFLOWS_BASE_URL"],
)
response = client.chat.completions.create(
    model=${JSON.stringify(model)},
    messages=[{"role": "user", "content": "Hello"}],
)
print(response.choices[0].message.content)`
  }

  return `const response = await fetch(${JSON.stringify(url)}, {
  method: "POST",
  headers: {
    "Content-Type": "application/json",
    ${protocol === 'anthropic' ? '"x-api-key": process.env.ANYFLOWS_API_KEY,' : 'Authorization: `Bearer ${process.env.ANYFLOWS_API_KEY}`,'}
    ${protocol === 'anthropic' ? '"anthropic-version": "2023-06-01",' : ''}
  },
  body: JSON.stringify(${payload}),
});

if (!response.ok) throw new Error(\`HTTP \${response.status}: \${await response.text()}\`);
console.log(await response.json());`
}

export function IssuedApiKeyOnboarding() {
  const { t } = useTranslation()
  const [search, setSearch] = useState('')
  const catalog = useModelCatalog(search, EMPTY_MODEL_CATALOG_FILTERS, true)
  const models = useMemo(() => catalog.data?.pages.flatMap((page) => page.models) ?? [], [catalog.data])
  const routableModels = models.filter((model) => modelProtocols(model).length > 0)
  const [selectedModel, setSelectedModel] = useState('')
  const model = routableModels.find((item) => item.model === selectedModel) ?? routableModels[0]
  const protocols = modelProtocols(model)
  const [selectedProtocol, setSelectedProtocol] = useState<SupportedProtocol>()
  const protocol = protocols.includes(selectedProtocol as SupportedProtocol) ? selectedProtocol as SupportedProtocol : protocols[0]
  const [language, setLanguage] = useState<SampleLanguage>('curl')
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle')
  const endpoint = useMemo(() => new URL(apiUrl('/v1'), window.location.origin).toString().replace(/\/$/, ''), [])
  const sample = model && protocol ? makeSample(language, protocol, model.model, endpoint) : ''

  return <section className="grid gap-3 border-t border-[var(--hairline)] pt-4" aria-label={t('apiKeys.issued.onboarding.title')}>
    <div className="grid gap-1"><h3 className="text-sm font-semibold">{t('apiKeys.issued.onboarding.title')}</h3><p className="text-xs leading-5 text-muted-foreground">{t('apiKeys.issued.onboarding.hint')}</p></div>
    <Input size="sm" aria-label={t('apiKeys.issued.onboarding.search')} placeholder={t('apiKeys.issued.onboarding.search')} value={search} onValueChange={setSearch} />
    <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_12rem]">
      <select aria-label={t('apiKeys.issued.onboarding.model')} className="h-9 min-w-0 rounded-lg border border-[var(--hairline)] bg-background px-3 text-sm" value={model?.model ?? ''} onChange={(event) => { setSelectedModel(event.target.value); setSelectedProtocol(undefined) }}>
        {routableModels.map((item) => <option key={item.model} value={item.model}>{item.display_name} · {item.model}</option>)}
      </select>
      <select aria-label={t('apiKeys.issued.onboarding.protocol')} className="h-9 min-w-0 rounded-lg border border-[var(--hairline)] bg-background px-3 text-sm" value={protocol ?? ''} onChange={(event) => setSelectedProtocol(event.target.value as SupportedProtocol)}>
        {protocols.map((item) => <option key={item} value={item}>{t(`apiKeys.issued.onboarding.protocols.${item}`)}</option>)}
      </select>
    </div>
    {catalog.hasNextPage ? <Button size="sm" variant="light" isDisabled={catalog.isFetchingNextPage} onPress={() => void catalog.fetchNextPage()}>{catalog.isFetchingNextPage ? t('apiKeys.issued.onboarding.loading') : t('apiKeys.issued.onboarding.moreModels')}</Button> : null}
    {catalog.isError ? <div role="alert" className="flex flex-wrap items-center justify-between gap-2 text-xs text-danger"><span>{t('apiKeys.issued.onboarding.catalogError')}</span><Button size="sm" variant="flat" isDisabled={catalog.isFetching} onPress={() => void catalog.refetch()}>{t('apiKeys.actions.retry')}</Button></div> : null}
    {sample ? <div className="grid gap-2 rounded-lg border border-[var(--hairline)] bg-surface-2/35 p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div role="group" aria-label={t('apiKeys.issued.onboarding.sampleLanguage')} className="inline-flex rounded-md border border-[var(--hairline)] bg-background p-0.5">
          {(['curl', 'python', 'javascript'] as const).map((item) => <button key={item} type="button" aria-pressed={language === item} onClick={() => { setLanguage(item); setCopyState('idle') }} className={`min-h-7 rounded px-2 text-xs ${language === item ? 'bg-primary/10 text-primary' : 'text-muted-foreground'}`}>{t(`apiKeys.issued.onboarding.languages.${item}`)}</button>)}
        </div>
        <Button size="sm" variant="flat" onPress={async () => setCopyState(await copyText(sample) ? 'copied' : 'failed')}>{t(copyState === 'copied' ? 'apiKeys.issued.onboarding.copied' : 'apiKeys.issued.onboarding.copy')}</Button>
      </div>
      <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-md bg-background p-3 font-mono text-[11px] leading-5">{sample}</pre>
      <p className="text-[11px] leading-4 text-muted-foreground">{t('apiKeys.issued.onboarding.placeholderNote')}</p>
      {copyState === 'failed' ? <p role="alert" className="text-xs text-danger">{t('apiKeys.issued.copyFailed')}</p> : null}
    </div> : <p role={catalog.isPending ? 'status' : undefined} className="rounded-lg border border-dashed border-[var(--hairline)] p-3 text-xs text-muted-foreground">{catalog.isPending ? t('apiKeys.issued.onboarding.loading') : t('apiKeys.issued.onboarding.noModels')}</p>}
    <Link className="w-fit text-xs font-medium text-primary hover:underline" to="/console/playground">{t('apiKeys.issued.playground')}</Link>
  </section>
}
