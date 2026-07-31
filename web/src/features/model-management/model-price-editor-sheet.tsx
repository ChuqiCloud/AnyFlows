import { useTranslation } from 'react-i18next'

import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type {
  AdminModel,
  AdminModelPrice,
  AdminModelPriceSourceCandidate,
} from '@/lib/api/generated/types.gen'
import { ModelPriceForm } from './model-price-form'
import type { ModelPriceDraft, StagedModelPriceDraft } from './model-price-form-model'

export type ModelPriceEditorTarget = {
  candidate?: AdminModelPriceSourceCandidate
  model: AdminModel
  price?: AdminModelPrice
  staged?: StagedModelPriceDraft
}

type ModelPriceEditorSheetProps = {
  onOpenChange: (open: boolean) => void
  onStage: (model: string, draft: ModelPriceDraft) => void
  target?: ModelPriceEditorTarget
}

/** 使用独立 Sheet 承载五价编辑，关闭时由工作区保留已形成的草稿。 */
export function ModelPriceEditorSheet(props: ModelPriceEditorSheetProps) {
  const { t } = useTranslation()
  return (
    <Sheet open={props.target !== undefined} onOpenChange={props.onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-2xl" aria-describedby="model-price-editor-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t('modelManagement.prices.editor.title')}</SheetTitle>
          <SheetDescription id="model-price-editor-description">
            {props.target
              ? t('modelManagement.prices.editor.description', { model: props.target.model.model })
              : t('modelManagement.prices.editor.fallbackDescription')}
          </SheetDescription>
        </SheetHeader>
        {props.target ? (
          <ModelPriceForm
            candidate={props.target.candidate}
            model={props.target.model}
            price={props.target.price}
            staged={props.target.staged}
            onCancel={() => props.onOpenChange(false)}
            onStage={(draft) => props.onStage(props.target!.model.model, draft)}
          />
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
