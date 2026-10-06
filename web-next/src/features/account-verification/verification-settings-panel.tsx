import { useEffect, useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, Input, Switch, Textarea } from '@heroui/react'
import { RefreshCw, Save } from 'lucide-react'

import { apiClient, ApiError } from '@/lib/api'
import { jsonBodySerializer } from '@/lib/api/generated/client'
import { sessionSecurity, verificationGet } from './account-verification-api'

type Settings = { source: 'environment' | 'database'; manual_enabled: boolean; enabled: boolean; app_id: string | null;
  individual_manual_enabled: boolean; enterprise_manual_enabled: boolean; individual_reason_required: boolean; enterprise_reason_required: boolean;
  private_key_configured: boolean; public_key_configured: boolean; gateway_url: string;
  biz_code: string; timeout_secs: number; version: number }
type Draft = { manual_enabled: boolean; individual_manual_enabled: boolean; enterprise_manual_enabled: boolean; individual_reason_required: boolean; enterprise_reason_required: boolean; enabled: boolean; app_id: string; private_key: string; public_key: string;
  gateway_url: string; biz_code: string; timeout_secs: number }
type SettingsRequest = Omit<Draft, 'private_key' | 'public_key'> & {
  private_key?: string; public_key?: string; expected_version: number }
const settingsKey = ['admin-account-verification-settings'] as const
const toDraft = (settings: Settings): Draft => ({ manual_enabled: settings.manual_enabled,
  individual_manual_enabled: settings.individual_manual_enabled ?? settings.manual_enabled,
  enterprise_manual_enabled: settings.enterprise_manual_enabled ?? settings.manual_enabled,
  individual_reason_required: settings.individual_reason_required ?? true,
  enterprise_reason_required: settings.enterprise_reason_required ?? true, enabled: settings.enabled,
  app_id: settings.app_id ?? '', private_key: '', public_key: '', gateway_url: settings.gateway_url,
  biz_code: settings.biz_code, timeout_secs: settings.timeout_secs })

export function VerificationSettingsPanel() {
  const client = useQueryClient()
  const query = useQuery({ queryKey: settingsKey, queryFn: ({ signal }) =>
    verificationGet<Settings>('/api/admin/account-verification-settings', signal), retry: false,
    refetchOnWindowFocus: false })
  const [draft, setDraft] = useState<Draft>()
  const [saved, setSaved] = useState(false)
  useEffect(() => { if (query.data) setDraft(toDraft(query.data)) }, [query.data])
  const mutation = useMutation({ mutationFn: async (body: SettingsRequest) => {
    const result = await apiClient.put<Settings>({ url: '/api/admin/account-verification-settings',
      security: [...sessionSecurity], body, bodySerializer: jsonBodySerializer.bodySerializer }) as unknown as { data: Settings }
    return result.data
  }, onSuccess: (settings) => { client.setQueryData(settingsKey, settings); setSaved(true); void client.invalidateQueries({ queryKey: ['account-verification', 'eligibility'] }) } })
  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!query.data || !draft) return
    setSaved(false)
    mutation.mutate({ ...draft, private_key: draft.private_key || undefined,
      public_key: draft.public_key || undefined, expected_version: query.data.version })
  }
  if (query.isPending) return <p role="status" className="text-sm text-default-500">正在读取认证配置…</p>
  if (query.isError || !query.data || !draft) return <div role="alert" className="flex items-center gap-3 text-sm text-danger">无法读取认证配置。<Button size="sm" variant="flat" onPress={() => void query.refetch()}><RefreshCw className="size-4" />重试</Button></div>
  const environment = query.data.source === 'environment'
  return <section className="grid gap-5">
    <header><h2 className="text-lg font-semibold">实名认证配置</h2><p className="mt-1 text-sm text-default-500">在这里控制用户可选择的认证方式。关闭某项后，用户提交页面不会再显示该方式。</p></header>
    {environment ? <p className="border-l-2 border-warning px-3 text-sm text-default-500">当前使用启动配置。首次在线保存后以后台设置为准；若启用支付宝，请重新填写两把密钥。</p> : null}
    <form onSubmit={submit} className="grid max-w-3xl gap-4">
      <section className="grid gap-3 rounded-xl border border-default-200 bg-content1 p-4"><h3 className="font-medium">个人认证</h3><Switch aria-label="个人认证人工审核" isSelected={draft.individual_manual_enabled} onValueChange={(individual_manual_enabled) => setDraft({ ...draft, individual_manual_enabled })}>启用人工审核</Switch><Switch aria-label="个人认证支付宝" isSelected={draft.enabled} onValueChange={(enabled) => setDraft({ ...draft, enabled })}>启用支付宝实名认证</Switch><Switch aria-label="个人认证理由必填" isSelected={draft.individual_reason_required} onValueChange={(individual_reason_required) => setDraft({ ...draft, individual_reason_required })}>要求填写认证理由</Switch></section>
      <section className="grid gap-3 rounded-xl border border-default-200 bg-content1 p-4"><h3 className="font-medium">企业认证</h3><Switch aria-label="企业认证人工审核" isSelected={draft.enterprise_manual_enabled} onValueChange={(enterprise_manual_enabled) => setDraft({ ...draft, enterprise_manual_enabled })}>启用人工审核</Switch><Switch aria-label="企业认证理由必填" isSelected={draft.enterprise_reason_required} onValueChange={(enterprise_reason_required) => setDraft({ ...draft, enterprise_reason_required })}>要求填写认证理由</Switch></section>
      <Input label="应用 ID" value={draft.app_id} maxLength={128} onValueChange={(app_id) => setDraft({ ...draft, app_id })} />
      <div className="grid gap-4 md:grid-cols-2">
        <Textarea label="应用私钥" description={`${!environment && query.data.private_key_configured ? '已配置，留空保持不变；' : ''}支持支付宝 Base64 原文或完整 PEM`} value={draft.private_key} minRows={6} autoComplete="off" onValueChange={(private_key) => setDraft({ ...draft, private_key })} />
        <Textarea label="支付宝公钥" description={`${!environment && query.data.public_key_configured ? '已配置，留空保持不变；' : ''}支持支付宝 Base64 原文或完整 PEM`} value={draft.public_key} minRows={6} autoComplete="off" onValueChange={(public_key) => setDraft({ ...draft, public_key })} />
      </div>
      <Input type="url" label="网关地址" isRequired maxLength={2048} value={draft.gateway_url} onValueChange={(gateway_url) => setDraft({ ...draft, gateway_url })} />
      <div className="grid gap-4 sm:grid-cols-2"><Input label="业务码" isRequired maxLength={64} value={draft.biz_code} onValueChange={(biz_code) => setDraft({ ...draft, biz_code })} /><Input type="number" label="请求超时（秒）" isRequired min={1} max={30} value={String(draft.timeout_secs)} onValueChange={(value) => setDraft({ ...draft, timeout_secs: Number(value) })} /></div>
      {mutation.isError ? <p role="alert" className="text-sm text-danger">{mutation.error instanceof ApiError && mutation.error.status === 409 ? '配置已被其他管理员更新，请刷新后重试。' : '保存失败，请检查密钥格式和配置项。'}</p> : null}
      {saved ? <p role="status" className="text-sm text-success">配置已保存并生效。</p> : null}
      <div className="flex gap-2"><Button type="submit" color="primary" isLoading={mutation.isPending}><Save className="size-4" />保存配置</Button><Button type="button" variant="flat" onPress={() => { void query.refetch(); mutation.reset(); setSaved(false) }}><RefreshCw className="size-4" />刷新</Button></div>
    </form>
  </section>
}
