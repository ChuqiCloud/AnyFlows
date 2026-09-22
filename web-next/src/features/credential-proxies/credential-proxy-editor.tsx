import { Button, Drawer, DrawerContent, DrawerFooter, DrawerHeader, Input, Select, SelectItem, Switch } from '@heroui/react'
import { useEffect, useState, type FormEvent, type ReactNode } from 'react'
import { Check, Eye, EyeOff, LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

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

/** 代理协议的可选值，顺序与原原生选项一致。 */
const SCHEME_KEYS: readonly AdminCredentialProxyScheme[] = ['http', 'https', 'socks5', 'socks5h']

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

  const schemeItems = SCHEME_KEYS.map((scheme) => ({ key: scheme, label: scheme.toUpperCase() }))

  return (
    <Drawer
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-[34rem]' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={handleOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] px-5 py-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t(proxy ? 'credentialProxies.editor.editTitle' : 'credentialProxies.editor.createTitle')}</h2>
              <p className="text-sm text-muted-foreground">{t(proxy ? 'credentialProxies.editor.editDescription' : 'credentialProxies.editor.createDescription')}</p>
            </DrawerHeader>
            <form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => void submit(event)}>
              <div className="grid min-h-0 flex-1 gap-5 overflow-y-auto px-5 py-4">
                <section className="grid gap-3">
                  <div><h3 className="text-sm font-semibold">{t('credentialProxies.editor.routeTitle')}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('credentialProxies.editor.routeDescription')}</p></div>
                  <Field label={t('credentialProxies.fields.name')} htmlFor="proxy-name"><Input autoFocus id="proxy-name" isDisabled={busy} maxLength={128} size="sm" value={form.name} onChange={(event) => update('name', event.target.value)} /></Field>
                  <div className="grid gap-3 sm:grid-cols-[0.8fr_1.4fr_0.7fr]">
                    <Field label={t('credentialProxies.fields.scheme')} htmlFor="proxy-scheme">
                      <Select
                        aria-label={t('credentialProxies.fields.scheme')}
                        id="proxy-scheme"
                        isDisabled={busy}
                        items={schemeItems}
                        selectedKeys={[form.scheme]}
                        size="sm"
                        onSelectionChange={(keys) => update('scheme', String(Array.from(keys)[0] ?? 'http') as AdminCredentialProxyScheme)}
                      >
                        {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                      </Select>
                    </Field>
                    <Field label={t('credentialProxies.fields.host')} htmlFor="proxy-host"><Input id="proxy-host" isDisabled={busy} maxLength={255} placeholder="proxy.example.com" size="sm" value={form.host} onChange={(event) => update('host', event.target.value)} /></Field>
                    <Field label={t('credentialProxies.fields.port')} htmlFor="proxy-port"><Input id="proxy-port" inputMode="numeric" isDisabled={busy} max={65_535} min={1} size="sm" type="number" value={form.port} onChange={(event) => update('port', event.target.value)} /></Field>
                  </div>
                </section>

                <section className="grid gap-3 border-t border-[var(--hairline)] pt-5">
                  <div><h3 className="text-sm font-semibold">{t('credentialProxies.editor.authTitle')}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{t(proxy?.password_configured ? 'credentialProxies.editor.authKeepHint' : 'credentialProxies.editor.authDescription')}</p></div>
                  <Field label={t('credentialProxies.fields.username')} htmlFor="proxy-username"><Input autoComplete="off" id="proxy-username" isDisabled={busy} maxLength={320} size="sm" value={form.username} onChange={(event) => update('username', event.target.value)} /></Field>
                  <Field label={t('credentialProxies.fields.password')} htmlFor="proxy-password">
                    <div className="relative">
                      <Input autoComplete="new-password" className="pr-10" id="proxy-password" isDisabled={busy || !form.username} maxLength={4096} size="sm" type={passwordVisible ? 'text' : 'password'} value={form.password} onChange={(event) => update('password', event.target.value)} />
                      <Button isIconOnly aria-label={t(passwordVisible ? 'credentialProxies.actions.hidePassword' : 'credentialProxies.actions.showPassword')} className="absolute right-1.5 top-1/2 size-6 min-w-6 -translate-y-1/2" isDisabled={!form.username} size="sm" type="button" variant="light" onClick={() => setPasswordVisible((value) => !value)}>{passwordVisible ? <EyeOff className="size-3" aria-hidden="true" /> : <Eye className="size-3" aria-hidden="true" />}</Button>
                    </div>
                  </Field>
                </section>

                <section className="grid gap-3 border-t border-[var(--hairline)] pt-5">
                  <ToggleRow id="proxy-enabled" label={t('credentialProxies.fields.enabled')} hint={t('credentialProxies.fields.enabledHint')} checked={form.enabled} disabled={busy} onCheckedChange={(value) => update('enabled', value)} />
                  <ToggleRow id="proxy-dns" label={t('credentialProxies.fields.trustProxyDns')} hint={t('credentialProxies.fields.trustProxyDnsHint')} checked={form.trustProxyDns} disabled={busy} onCheckedChange={(value) => update('trustProxyDns', value)} />
                </section>
              </div>
              <DrawerFooter className="border-t border-[var(--hairline)] px-5 py-3">
                <div className="flex min-h-8 flex-1 items-center justify-between gap-3">
                  <p className="text-xs text-destructive" role="alert">{validationError ? t('credentialProxies.errors.invalid') : createMutation.isError || updateMutation.isError ? t('credentialProxies.errors.save') : ''}</p>
                  <Button type="submit" color="primary" isDisabled={busy}>{busy ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Check className="size-4" aria-hidden="true" />}{t(proxy ? 'credentialProxies.actions.save' : 'credentialProxies.actions.create')}</Button>
                </div>
              </DrawerFooter>
            </form>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}

function Field({ label, htmlFor, children }: { label: string; htmlFor: string; children: ReactNode }) {
  {/* HeroUI 没有独立的 Label 组件，表单标签保留原生元素以维持标签关联。 */}
  return <div className="grid gap-1.5"><label className="text-xs font-medium leading-none text-foreground" htmlFor={htmlFor}>{label}</label>{children}</div>
}

function ToggleRow({ id, label, hint, checked, disabled, onCheckedChange }: { id: string; label: string; hint: string; checked: boolean; disabled: boolean; onCheckedChange: (value: boolean) => void }) {
  return <div className="flex items-start justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5"><div><label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label><p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p></div><Switch id={id} isDisabled={disabled} isSelected={checked} size="sm" onValueChange={onCheckedChange} /></div>
}
