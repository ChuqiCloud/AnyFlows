import { useState } from 'react'
import { Button, Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { CredentialUsageConfig } from './credential-usage-config'

export function ChannelUsageSettings({ channelId }: { channelId: number }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  return <><Button type="button" size="sm" variant="bordered" onPress={() => setOpen(true)}>{t('keyCapabilities.channelUsageDefaults')}</Button>{open ? <Drawer isOpen size="2xl" onOpenChange={(value) => setOpen(value)}><DrawerContent className="min-w-0 bg-surface-1 text-foreground"><DrawerHeader className="flex items-center justify-between gap-3 pr-12"><span>{t('keyCapabilities.channelUsageDefaults')}</span><Button type="button" isIconOnly size="sm" variant="light" aria-label={t('keyCapabilities.close')} onPress={() => setOpen(false)}><X className="size-4" /></Button></DrawerHeader><DrawerBody className="pb-6"><CredentialUsageConfig channelId={channelId} /></DrawerBody></DrawerContent></Drawer> : null}</>
}
