import { useEffect, useState, type FormEvent, type ReactNode } from 'react'
import { Check, Eye, EyeOff, LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Switch } from '@/components/ui/switch'
import {
  useCreateAdminCredentialProxy,
  useUpdateAdminCredentialProxy,
  type AdminCredentialProxy,
  type AdminCredentialProxyScheme,
  type AdminCredentialProxyWriteRequest,
} from './credential-proxy-api'

type ProxyForm = {
  name: string
  scheme: AdminCredentialProxyScheme
  host: string
  port: string
  username: string
  password: string
  trustProxyDns: boolean
  enabled: boolean
}

const emptyForm: ProxyForm = {
  name: '',
  scheme: 'http',
  host: '',
  port: '8080',
  username: '',
  password: '',
  trustProxyDns: false,
  enabled: true,
}

export function CredentialProxyEditor({ open, proxy, onOpenChange, onSaved }: {
  open: boolean
  proxy?: AdminCredentialProxy
  onOpenChange: (open: boolean) => void
  onSaved: () => void
}) {
  const { t } = useTranslation()
  const [form, setForm] = useState<ProxyForm>(emptyForm)
  const [passwordVisible, setPasswordVisible] = useState(false)
  const [validationError, setValidationError] = useState(false)
  const createMutation = useCreateAdminCredentialProxy()
  const updateMutation = useUpdateAdminCredentialProxy()
  const busy = createMutation.isPending || updateMutation.isPending

  useEffect(() => {
    if (!open) return
    setForm(proxy ? {
      name: proxy.name,
      scheme: proxy.scheme,
      host: proxy.host,
      port: String(proxy.port),
      username: proxy.username ?? '',
      password: '',
      trustProxyDns: proxy.trust_proxy_dns,
      enabled: proxy.enabled,
    } : emptyForm)
    setPasswordVisible(false)
    setValidationError(false)
  }, [open, proxy])

  const resetMutations = () => {
    createMutation.reset()
    updateMutation.reset()
  }

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) resetMutations()
    onOpenChange(nextOpen)
  }

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    const port = Number(form.port)
    const username = form.username.trim()
    if (!form.name.trim() || !form.host.trim() || !Number.isInteger(port) || port < 1 || port > 65_535
      || (username !== '' && form.password === '' && !proxy?.password_configured)) {
      setValidationError(true)
      return
    }
    const body: AdminCredentialProxyWriteRequest = {
      name: form.name.trim(),
      scheme: form.scheme,
      host: form.host.trim(),
      port,
      username: username || null,
      password: username && form.password ? form.password : null,
      trust_proxy_dns: form.trustProxyDns,
      enabled: form.enabled,
    }
    try {
      if (proxy) await updateMutation.mutateAsync({ id: proxy.id, body })
      else await createMutation.mutateAsync(body)
      resetMutations()
      onSaved()
    } catch {
      // 表单保留非敏感字段与本次密码输入，错误只显示固定文案供管理员重试。
    }
  }

  const update = <K extends keyof ProxyForm>(key: K, value: ProxyForm[K]) => {
    setValidationError(false)
    setForm((current) => ({ ...current, [key]: value }))
  }

  return (
    <Sheet open={open} onOpenChange={handleOpenChange}>
      <SheetContent className="w-[min(100vw,34rem)] sm:max-w-[34rem]">
        <SheetHeader className="border-b border-[var(--hairline)] px-5 py-4">
          <SheetTitle>{t(proxy ? 'credentialProxies.editor.editTitle' : 'credentialProxies.editor.createTitle')}</SheetTitle>
          <SheetDescription>{t(proxy ? 'credentialProxies.editor.editDescription' : 'credentialProxies.editor.createDescription')}</SheetDescription>
        </SheetHeader>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => void submit(event)}>
          <div className="grid min-h-0 flex-1 gap-5 overflow-y-auto px-5 py-4">
            <section className="grid gap-3">
              <div><h3 className="text-sm font-semibold">{t('credentialProxies.editor.routeTitle')}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('credentialProxies.editor.routeDescription')}</p></div>
              <Field label={t('credentialProxies.fields.name')} htmlFor="proxy-name"><Input id="proxy-name" value={form.name} disabled={busy} maxLength={128} autoFocus onChange={(event) => update('name', event.target.value)} /></Field>
              <div className="grid gap-3 sm:grid-cols-[0.8fr_1.4fr_0.7fr]">
                <Field label={t('credentialProxies.fields.scheme')} htmlFor="proxy-scheme"><Select id="proxy-scheme" value={form.scheme} disabled={busy} onChange={(event) => update('scheme', event.target.value as AdminCredentialProxyScheme)}>{['http', 'https', 'socks5', 'socks5h'].map((scheme) => <option key={scheme} value={scheme}>{scheme.toUpperCase()}</option>)}</Select></Field>
                <Field label={t('credentialProxies.fields.host')} htmlFor="proxy-host"><Input id="proxy-host" value={form.host} disabled={busy} maxLength={255} placeholder="proxy.example.com" onChange={(event) => update('host', event.target.value)} /></Field>
                <Field label={t('credentialProxies.fields.port')} htmlFor="proxy-port"><Input id="proxy-port" type="number" min={1} max={65_535} value={form.port} disabled={busy} onChange={(event) => update('port', event.target.value)} /></Field>
              </div>
            </section>

            <section className="grid gap-3 border-t border-[var(--hairline)] pt-5">
              <div><h3 className="text-sm font-semibold">{t('credentialProxies.editor.authTitle')}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{t(proxy?.password_configured ? 'credentialProxies.editor.authKeepHint' : 'credentialProxies.editor.authDescription')}</p></div>
              <Field label={t('credentialProxies.fields.username')} htmlFor="proxy-username"><Input id="proxy-username" value={form.username} disabled={busy} maxLength={320} autoComplete="off" onChange={(event) => update('username', event.target.value)} /></Field>
              <Field label={t('credentialProxies.fields.password')} htmlFor="proxy-password">
                <div className="relative">
                  <Input id="proxy-password" className="pr-10" type={passwordVisible ? 'text' : 'password'} value={form.password} disabled={busy || !form.username} maxLength={4096} autoComplete="new-password" onChange={(event) => update('password', event.target.value)} />
                  <Button type="button" size="icon-xs" variant="ghost" className="absolute right-1.5 top-1/2 -translate-y-1/2" disabled={!form.username} aria-label={t(passwordVisible ? 'credentialProxies.actions.hidePassword' : 'credentialProxies.actions.showPassword')} onClick={() => setPasswordVisible((value) => !value)}>{passwordVisible ? <EyeOff aria-hidden="true" /> : <Eye aria-hidden="true" />}</Button>
                </div>
              </Field>
            </section>

            <section className="grid gap-3 border-t border-[var(--hairline)] pt-5">
              <ToggleRow id="proxy-enabled" label={t('credentialProxies.fields.enabled')} hint={t('credentialProxies.fields.enabledHint')} checked={form.enabled} disabled={busy} onCheckedChange={(value) => update('enabled', value)} />
              <ToggleRow id="proxy-dns" label={t('credentialProxies.fields.trustProxyDns')} hint={t('credentialProxies.fields.trustProxyDnsHint')} checked={form.trustProxyDns} disabled={busy} onCheckedChange={(value) => update('trustProxyDns', value)} />
            </section>
          </div>
          <SheetFooter className="border-t border-[var(--hairline)] px-5 py-3">
            <div className="flex min-h-8 items-center justify-between gap-3">
              <p className="text-xs text-destructive" role="alert">{validationError ? t('credentialProxies.errors.invalid') : createMutation.isError || updateMutation.isError ? t('credentialProxies.errors.save') : ''}</p>
              <Button type="submit" disabled={busy}>{busy ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Check aria-hidden="true" />}{t(proxy ? 'credentialProxies.actions.save' : 'credentialProxies.actions.create')}</Button>
            </div>
          </SheetFooter>
        </form>
      </SheetContent>
    </Sheet>
  )
}

function Field({ label, htmlFor, children }: { label: string; htmlFor: string; children: ReactNode }) {
  return <div className="grid gap-1.5"><Label htmlFor={htmlFor}>{label}</Label>{children}</div>
}

function ToggleRow({ id, label, hint, checked, disabled, onCheckedChange }: { id: string; label: string; hint: string; checked: boolean; disabled: boolean; onCheckedChange: (value: boolean) => void }) {
  return <div className="flex items-start justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5"><div><Label htmlFor={id}>{label}</Label><p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p></div><Switch id={id} checked={checked} disabled={disabled} onCheckedChange={onCheckedChange} /></div>
}
