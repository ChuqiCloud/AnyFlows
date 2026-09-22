import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

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

/** 使用独立 Drawer 承载固定价格与表达式编辑，关闭时由工作区保留已形成的草稿。 */
export function ModelPriceEditorSheet(props: ModelPriceEditorSheetProps) {
  const { t } = useTranslation()
  const target = props.target
  return (
    <Drawer
      aria-describedby="model-price-editor-description"
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-2xl' }}
      isOpen={target !== undefined}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={props.onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t('modelManagement.prices.editor.title')}</h2>
              <p className="text-sm text-muted-foreground" id="model-price-editor-description">
                {target
                  ? t('modelManagement.prices.editor.description', { model: target.model.model })
                  : t('modelManagement.prices.editor.fallbackDescription')}
              </p>
            </DrawerHeader>
            {/* 表单自带滚动区与底栏，占满剩余高度。 */}
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
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
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
