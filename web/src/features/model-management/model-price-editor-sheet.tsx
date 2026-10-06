import { useTranslation } from 'react-i18next'

import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type {
  AdminModel,
  AdminModelPrice,
  AdminModelPriceSourceCandidate,
} from '@/lib/api/generated/types.gen'
import { ModelPriceForm } from './model-price-form'
import type { ModelPriceSource } from './model-price-api'
import type { ModelPriceDraft, StagedModelPriceDraft } from './model-price-form-model'

export type ModelPriceEditorTarget = {
  candidate?: AdminModelPriceSourceCandidate
  model: AdminModel
  price?: AdminModelPrice
  source?: ModelPriceSource
  staged?: StagedModelPriceDraft
}

type ModelPriceEditorSheetProps = {
  onOpenChange: (open: boolean) => void
  onStage: (model: string, draft: ModelPriceDraft, expectedVersion: number | null, contextWindow: number | null) => void
  target?: ModelPriceEditorTarget
}

/** 使用独立 Sheet 承载固定价格与表达式编辑，关闭时由工作区保留已形成的草稿。 */
export function ModelPriceEditorSheet(props: ModelPriceEditorSheetProps) {
  const { t } = useTranslation()
  const target = props.target
  return (
    <Sheet open={target !== undefined} onOpenChange={props.onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-2xl" aria-describedby="model-price-editor-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t('modelManagement.prices.editor.title')}</SheetTitle>
          <SheetDescription id="model-price-editor-description">
            {target
              ? t('modelManagement.prices.editor.description', { model: target.model.model })
              : t('modelManagement.prices.editor.fallbackDescription')}
          </SheetDescription>
        </SheetHeader>
        {target ? (
          <ModelPriceForm
            candidate={target.candidate}
            model={target.model}
            price={target.price}
            source={target.source}
            staged={target.staged}
            onCancel={() => props.onOpenChange(false)}
            onStage={(draft, expectedVersion, contextWindow) => props.onStage(target.model.model, draft, expectedVersion, contextWindow)}
          />
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
