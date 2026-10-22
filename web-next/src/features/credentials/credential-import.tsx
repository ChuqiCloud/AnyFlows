import { useState, type ChangeEvent } from 'react'
import { FileUp, LoaderCircle } from 'lucide-react'
import { useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { Button } from '@heroui/react'
import type { AdminChannel } from '@/lib/api/generated/types.gen'
import { adminChannelCredentialsQueryKey, importAdminCredentialFiles, type AdminCredentialImportResult } from './credential-api'
import { credentialKindsForChannel } from './credential-model'

export function CredentialImport({ channel, onImported }: { channel: AdminChannel; onImported: (result: AdminCredentialImportResult) => Promise<void> }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const kinds = credentialKindsForChannel(channel.type, channel.provider)
  const oauth = channel.type === 'openai' && channel.protocol === 'openai_responses' && kinds.includes('oauth')
  const apiKey = kinds.includes('api_key')
  const [kind, setKind] = useState<'api_key' | 'oauth'>(oauth && !apiKey ? 'oauth' : 'api_key')
  const [text, setText] = useState('')
  const [pending, setPending] = useState(false)
  const [error, setError] = useState(false)
  if (!apiKey && !oauth) return null
  async function run(files: Array<{ name: string; content: string }>) {
    setError(false)
    const bytes = files.map((file) => new TextEncoder().encode(file.content).length)
    if (files.length > 64 || bytes.some((size) => size > 2 * 1024 * 1024) || bytes.reduce((sum, size) => sum + size, 0) > 24 * 1024 * 1024) { setError(true); return }
    setPending(true)
    try {
      const result = await importAdminCredentialFiles(channel.id, files, kind)
      setText('')
      await queryClient.invalidateQueries({ queryKey: adminChannelCredentialsQueryKey(channel.id) })
      await onImported(result)
    } catch { setError(true) } finally { setPending(false) }
  }
  async function importFiles(event: ChangeEvent<HTMLInputElement>) {
    const files = Array.from(event.target.files ?? [])
    event.target.value = ''
    if (!files.length) return
    if (files.length > 64 || files.some((file) => file.size > 2 * 1024 * 1024) || files.reduce((sum, file) => sum + file.size, 0) > 24 * 1024 * 1024) { setError(true); return }
    await run(await Promise.all(files.map(async (file) => ({ name: file.name, content: await file.text() }))))
  }
  return <details className="border-b border-[var(--hairline)] pb-3">
    <summary className="cursor-pointer text-sm font-medium">{t('credentials.batch.importTitle', { defaultValue: '批量导入' })}</summary>
    <div className="mt-3 grid gap-3">
      <label className="flex items-center gap-2 text-xs">{t('credentials.batch.kind', { defaultValue: '凭据类型' })}<select value={kind} disabled={pending} onChange={(event) => setKind(event.target.value as 'api_key' | 'oauth')} className="rounded-md border border-[var(--hairline)] bg-transparent p-2">
        {apiKey ? <option value="api_key">API Key</option> : null}{oauth ? <option value="oauth">OAuth / Codex</option> : null}
      </select></label>
      <textarea aria-label={t('credentials.batch.input', { defaultValue: '逐行 Key 或 JSON' })} autoComplete="off" spellCheck={false} value={text} disabled={pending} onChange={(event) => setText(event.target.value)} rows={4} className="w-full rounded-md border border-[var(--hairline)] bg-transparent p-3 font-mono text-xs" placeholder={t('credentials.batch.placeholder', { defaultValue: '每行一个 Key，或粘贴 JSON 数组、keys / credentials 对象。' })} />
      <div className="flex flex-wrap items-center gap-3">
        <Button type="button" size="sm" isDisabled={pending || !text.trim()} onClick={() => void run([{ name: 'pasted-credentials.txt', content: text }])}>{pending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <FileUp className="size-4" aria-hidden="true" />}{t('credentials.fileTransfer.import')}</Button>
        <label className="cursor-pointer text-sm underline"><span>{t('credentials.batch.files', { defaultValue: '选择文件' })}</span><input type="file" multiple accept=".json,.jsonl,.txt" className="sr-only" disabled={pending} onChange={(event) => void importFiles(event)} /></label>
      </div>
      {error ? <p role="alert" className="text-xs text-destructive">{t('credentials.fileTransfer.importFailed')}</p> : null}
    </div>
  </details>
}
