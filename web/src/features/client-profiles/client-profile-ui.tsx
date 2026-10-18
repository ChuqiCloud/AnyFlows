import { Switch } from '@/components/ui/switch'
import { useEffect, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetDescription } from '@/components/ui/sheet'

export type ProfileButtonProps = { children: ReactNode; disabled?: boolean; onClick?: () => void; primary?: boolean; type?: 'button' | 'submit' }
export function ProfileButton({ primary, ...props }: ProfileButtonProps) {
  return <Button size="sm" variant={primary ? 'default' : 'secondary'} type="button" {...props} />
}
export type ProfileInputProps = { label: string; value: string; onChange: (value: string) => void; disabled?: boolean; maxLength?: number; placeholder?: string }
export function ProfileInput({ label, onChange, ...props }: ProfileInputProps) {
  return <Input aria-label={label} onChange={event => onChange(event.target.value)} {...props} />
}
export type ProfileSelectProps = { label: string; value: string; options: { value: string; label: string }[]; onChange: (value: string) => void; disabled?: boolean }
export function ProfileSelect({ label, options, onChange, ...props }: ProfileSelectProps) {
  return <Select aria-label={label} onChange={event => onChange(event.target.value)} {...props}>{options.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}</Select>
}
export type ProfileDrawerProps = { title: string; dirty?: boolean; busy?: boolean; children: ReactNode; onClose: () => void }
export function ProfileDrawer({ title, dirty, busy, children, onClose }: ProfileDrawerProps) {
  const { t } = useTranslation()
  const [discard, setDiscard] = useState(false)
  useEffect(() => {
    if (!dirty && !busy) return
    const preventUnload = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = '' }
    window.addEventListener('beforeunload', preventUnload)
    return () => window.removeEventListener('beforeunload', preventUnload)
  }, [dirty, busy])
  return <Sheet open onOpenChange={open => { if (!open && !busy) { if (dirty) setDiscard(true); else onClose() } }}>
    <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-2xl" aria-describedby="profile-drawer-description">
      <SheetHeader className="border-b border-[var(--hairline)] pr-12"><SheetTitle>{title}</SheetTitle><SheetDescription id="profile-drawer-description">{t('clientProfiles.versionHint')}</SheetDescription></SheetHeader>
      {discard && <div className="cp-discard" role="alert"><p>{t('clientProfiles.unsaved')}</p><ProfileButton onClick={() => setDiscard(false)}>{t('clientProfiles.keepEditing')}</ProfileButton><ProfileButton onClick={onClose}>{t('clientProfiles.discard')}</ProfileButton></div>}
      <div className="cp-drawer-body">{children}</div>
    </SheetContent>
  </Sheet>
}

export function ProfileSwitch({ label, checked, disabled, onChange }: { label: string; checked: boolean; disabled?: boolean; onChange: (value: boolean) => void }) {
  return <label className="cp-heading"><span>{label}</span><Switch aria-label={label} checked={checked} disabled={disabled} onCheckedChange={onChange} /></label>
}
