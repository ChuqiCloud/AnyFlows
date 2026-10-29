import { useEffect, useState } from 'react'
import { Button, Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

import type { AdminGroup } from '@/lib/api/generated/types.gen'
import { GroupForm } from './group-form'
import { GroupKeyPolicyEditor } from './group-key-policy'

type GroupEditorSheetProps = {
  group?: AdminGroup
  groups: AdminGroup[]
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function GroupEditorSheet({ group, groups, open, onOpenChange }: GroupEditorSheetProps) {
  const { t } = useTranslation()
  const mode = group ? 'update' : 'create'
  const [panel, setPanel] = useState<'details' | 'key-policy'>('details')
  const [portalContainer, setPortalContainer] = useState<HTMLDivElement | null>(null)
  useEffect(() => setPanel('details'), [group?.id, open])

  return (
    <Drawer
      aria-describedby="group-editor-description"
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-2xl' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <div ref={setPortalContainer} className="relative flex h-full min-h-0 flex-col">
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t(group ? 'groups.editor.editTitle' : 'groups.editor.createTitle')}</h2>
              <p className="text-sm text-muted-foreground" id="group-editor-description">{t(group ? 'groups.editor.editDescription' : 'groups.editor.createDescription')}</p>
            </DrawerHeader>
            {group ? (
              <div className="flex shrink-0 gap-2 border-b border-[var(--hairline)] px-4 py-2" role="group" aria-label={t('groups.keyPolicy.sectionLabel')}>
                <Button type="button" size="sm" variant={panel === 'details' ? 'flat' : 'light'} aria-pressed={panel === 'details'} onClick={() => setPanel('details')}>{t('groups.keyPolicy.detailsTab')}</Button>
                <Button type="button" size="sm" variant={panel === 'key-policy' ? 'flat' : 'light'} aria-pressed={panel === 'key-policy'} onClick={() => setPanel('key-policy')}>{t('groups.keyPolicy.keyPolicyTab')}</Button>
              </div>
            ) : null}
            {/* 表单自带滚动区与底栏，占满剩余高度。 */}
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
              <div className={!group || panel === 'details' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}>
                <GroupForm mode={mode} group={group} groups={groups} onCancel={() => onOpenChange(false)} onSaved={() => onOpenChange(false)} />
              </div>
              {group ? (
                <div className={panel === 'key-policy' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}>
                  <GroupKeyPolicyEditor key={group.id} groupId={group.id} portalContainer={portalContainer} />
                </div>
              ) : null}
            </DrawerBody>
          </div>
        )}
      </DrawerContent>
    </Drawer>
  )
}
