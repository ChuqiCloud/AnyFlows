import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { AdminGroup } from '@/lib/api/generated/types.gen'
import { GroupForm } from './group-form'
import { GroupKeyPolicyEditor } from './group-key-policy'
import { GroupModelPolicyEditor } from './group-model-policy'

type GroupEditorSheetProps = {
  group?: AdminGroup
  groups: AdminGroup[]
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function GroupEditorSheet({ group, groups, open, onOpenChange }: GroupEditorSheetProps) {
  const { t } = useTranslation()
  const mode = group ? 'update' : 'create'
  const [panel, setPanel] = useState<'details' | 'key-policy' | 'model-policy'>('details')
  useEffect(() => setPanel('details'), [group?.id, open])

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-2xl" aria-describedby="group-editor-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t(group ? 'groups.editor.editTitle' : 'groups.editor.createTitle')}</SheetTitle>
          <SheetDescription id="group-editor-description">{t(group ? 'groups.editor.editDescription' : 'groups.editor.createDescription')}</SheetDescription>
        </SheetHeader>
        {group ? (
          <div className="flex shrink-0 gap-2 border-b border-[var(--hairline)] px-4 py-2" role="group" aria-label={t('groups.keyPolicy.sectionLabel')}>
            <Button type="button" size="sm" variant={panel === 'details' ? 'default' : 'ghost'} aria-pressed={panel === 'details'} onClick={() => setPanel('details')}>{t('groups.keyPolicy.detailsTab')}</Button>
            <Button type="button" size="sm" variant={panel === 'key-policy' ? 'default' : 'ghost'} aria-pressed={panel === 'key-policy'} onClick={() => setPanel('key-policy')}>{t('groups.keyPolicy.keyPolicyTab')}</Button>
            <Button type="button" size="sm" variant={panel === 'model-policy' ? 'default' : 'ghost'} aria-pressed={panel === 'model-policy'} onClick={() => setPanel('model-policy')}>{t('groups.modelPolicy.tab')}</Button>
          </div>
        ) : null}
        <div className={!group || panel === 'details' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}>
          <GroupForm mode={mode} group={group} groups={groups} onCancel={() => onOpenChange(false)} onSaved={() => onOpenChange(false)} />
        </div>
        {group ? (
          <div className={panel === 'key-policy' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}>
            <GroupKeyPolicyEditor key={group.id} groupId={group.id} />
          </div>
        ) : null}
        {group ? (
          <div className={panel === 'model-policy' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}>
            <GroupModelPolicyEditor key={group.id} groupId={group.id} />
          </div>
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
