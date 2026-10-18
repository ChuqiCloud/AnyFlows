import { createContext, useContext, useEffect, useId, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Input, Select, SelectItem, Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'

const PortalContext = createContext<HTMLElement | null>(null)
export type ProfileButtonProps = { children: ReactNode; disabled?: boolean; onClick?: () => void; primary?: boolean; type?: 'button' | 'submit' }
export function ProfileButton({ primary, disabled, ...props }: ProfileButtonProps) {
  return <Button size="sm" color={primary ? 'primary' : 'default'} variant={primary ? 'solid' : 'bordered'} type="button" isDisabled={disabled} {...props} />
}
export type ProfileInputProps = { label: string; value: string; onChange: (value: string) => void; disabled?: boolean; maxLength?: number; placeholder?: string }
export function ProfileInput({ label, disabled, onChange, ...props }: ProfileInputProps) {
  return <Input aria-label={label} size="sm" variant="bordered" isDisabled={disabled} onValueChange={onChange} {...props} />
}
export type ProfileSelectProps = { label: string; value: string; options: { value: string; label: string }[]; onChange: (value: string) => void; disabled?: boolean }
export function ProfileSelect({ label, value, options, onChange, disabled }: ProfileSelectProps) {
  const portal = useContext(PortalContext)
  const labelId = useId()
  return <div className="min-w-0"><span id={labelId} className="sr-only">{label}</span><Select aria-label={label} aria-labelledby={labelId} size="sm" variant="bordered" isDisabled={disabled} selectedKeys={[value || '__all__']} items={options.map(option => ({ ...option, key: option.value || '__all__' }))} popoverProps={portal ? { portalContainer: portal } : undefined} onSelectionChange={keys => { const key = Array.from(keys)[0]; if (typeof key === 'string') onChange(key === '__all__' ? '' : key) }}>
    {option => <SelectItem key={option.key}>{option.label}</SelectItem>}
  </Select></div>
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
  const [portal, setPortal] = useState<HTMLDivElement | null>(null)
  return <Drawer isOpen backdrop="blur" placement="right" scrollBehavior="inside" classNames={{ base: 'w-full max-h-none sm:max-w-2xl' }} aria-describedby="profile-drawer-description" onOpenChange={open => { if (!open && !busy) { if (dirty) setDiscard(true); else onClose() } }}>
    <DrawerContent>{() => <div ref={setPortal} className="relative flex h-full min-h-0 flex-col"><PortalContext.Provider value={portal}>
      <DrawerHeader className="flex flex-col gap-1 border-b border-[var(--hairline)] p-5 pr-12"><h2 className="text-base font-semibold">{title}</h2><p className="text-sm font-normal text-muted-foreground" id="profile-drawer-description">{t('clientProfiles.versionHint')}</p></DrawerHeader>
      {discard && <div className="cp-discard" role="alert"><p>{t('clientProfiles.unsaved')}</p><ProfileButton onClick={() => setDiscard(false)}>{t('clientProfiles.keepEditing')}</ProfileButton><ProfileButton onClick={onClose}>{t('clientProfiles.discard')}</ProfileButton></div>}
      <DrawerBody className="flex min-h-0 flex-col gap-0 overflow-hidden p-0"><div className="cp-drawer-body">{children}</div></DrawerBody>
    </PortalContext.Provider></div>}</DrawerContent>
  </Drawer>
}
