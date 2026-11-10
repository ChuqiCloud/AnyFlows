import { useState } from 'react'
import { Button, Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { CredentialUsageConfig } from './credential-usage-config'

export function ChannelUsageSettings({ channelId }: { channelId: number }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  return <><Button type="button" size="sm" variant="bordered" onPress={() => setOpen(true)}>{t('keyCapabilities.channelUsageDefaults')}</Button>{open ? <Drawer isOpen size="2xl" onOpenChange={(value) => setOpen(value)}><DrawerContent className="min-w-0 bg-surface-1 text-foreground"><DrawerHeader className="pr-12">{t('keyCapabilities.channelUsageDefaults')}</DrawerHeader><DrawerBody className="pb-6"><CredentialUsageConfig channelId={channelId} /></DrawerBody></DrawerContent></Drawer> : null}</>
}
