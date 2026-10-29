import {
  Button,
  Drawer,
  DrawerBody,
  DrawerContent,
  DrawerHeader,
  Select,
  SelectItem,
  Textarea,
} from '@heroui/react'
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion'
import {
  AlertCircle,
  Check,
  Copy,
  Download,
  Image as ImageIcon,
  ImagePlus,
  LoaderCircle,
  Pencil,
  RefreshCw,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  X,
} from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { EMPTY_MODEL_CATALOG_FILTERS, useModelCatalog, type ModelCatalogFilters } from '@/features/models/model-api'
import type { ModelCatalogItem } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import {
  clearImageHistory,
  deleteImageHistory,
  editImage,
  generateImages,
  ImagePlazaRequestError,
  loadImageHistory,
  loadImageHistoryImage,
  saveImageHistory,
  type ImagePlazaHistoryRecord,
  type ImagePlazaResult,
  type ImagePlazaSettings,
} from './image-plaza-client'

type ImageMode = 'generate' | 'edit'
type PlazaImage = ImagePlazaResult & {
  model: string
  prompt: string
  createdAt: number
  historyId?: string
  historyLoaded?: boolean
  format?: ImagePlazaHistoryRecord['format']
}
type ImageCatalogQuery = ReturnType<typeof useModelCatalog>

const imageSizes = ['1024x1024', '1536x1024', '1024x1536']
const imageQualities = ['standard', 'hd']

export function ImagePlazaPage() {
  const { t } = useTranslation()
  const prefersReducedMotion = useReducedMotion()
  const filters = useMemo<ModelCatalogFilters>(
    () => ({ ...EMPTY_MODEL_CATALOG_FILTERS, outputModalities: ['image'] }),
    [],
  )
  const catalogQuery = useModelCatalog('', filters, true)
  const models = useMemo(
    () => catalogQuery.data?.pages.flatMap((page) => page.models) ?? [],
    [catalogQuery.data],
  )
  const [mode, setMode] = useState<ImageMode>('generate')
  const [model, setModel] = useState('')
  const [prompt, setPrompt] = useState('')
  const [size, setSize] = useState(imageSizes[0])
  const [quality, setQuality] = useState(imageQualities[0])
  const [count, setCount] = useState(1)
  const [originalImage, setOriginalImage] = useState<File>()
  const [maskImage, setMaskImage] = useState<File>()
  const [results, setResults] = useState<PlazaImage[]>([])
  const [selectedId, setSelectedId] = useState<string>()
  const [busy, setBusy] = useState(false)
  const [historySaving, setHistorySaving] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [errorKey, setErrorKey] = useState('')
  const [fileErrorKey, setFileErrorKey] = useState('')
  const [copied, setCopied] = useState(false)
  const [noticeKey, setNoticeKey] = useState('')
  const [sourcePreview, setSourcePreview] = useState('')
  const [maskPreview, setMaskPreview] = useState('')
  const resultsRef = useRef<PlazaImage[]>([])
  const previousResultsRef = useRef<PlazaImage[]>([])
  const selectedImage = results.find((item) => item.id === selectedId) ?? results[0]

  useEffect(() => {
    resultsRef.current = results
    const currentUrls = new Set(results.map((image) => image.src))
    for (const image of previousResultsRef.current) {
      if (image.blob && image.src.startsWith('blob:') && !currentUrls.has(image.src)) {
        URL.revokeObjectURL(image.src)
      }
    }
    previousResultsRef.current = results
  }, [results])

  useEffect(() => () => {
    for (const image of resultsRef.current) {
      if (image.blob && image.src.startsWith('blob:')) URL.revokeObjectURL(image.src)
    }
  }, [])

  useEffect(() => {
    let cancelled = false
    void loadImageHistory().then((history) => {
      if (cancelled) return
      const restored = history.map(historyImage)
      setResults((current) => {
        const ids = new Set(current.map((image) => image.historyId ?? image.id))
        return [...current, ...restored.filter((image) => !ids.has(image.id))]
          .sort((left, right) => right.createdAt - left.createdAt)
          .slice(0, 30)
      })
    }).catch(() => {
      if (!cancelled) setNoticeKey('historyLoadFailed')
    })
    return () => { cancelled = true }
  }, [])

  useEffect(() => {
    if (!noticeKey) return
    const timeout = window.setTimeout(() => setNoticeKey(''), 2500)
    return () => window.clearTimeout(timeout)
  }, [noticeKey])

  useEffect(() => {
    if (models.length && (!model || !models.some((item) => item.model === model))) {
      setModel(models[0].model)
    }
  }, [model, models])

  useEffect(() => {
    if (!originalImage) {
      setSourcePreview('')
      return
    }
    const url = URL.createObjectURL(originalImage)
    setSourcePreview(url)
    return () => URL.revokeObjectURL(url)
  }, [originalImage])

  useEffect(() => {
    if (!maskImage) {
      setMaskPreview('')
      return
    }
    const url = URL.createObjectURL(maskImage)
    setMaskPreview(url)
    return () => URL.revokeObjectURL(url)
  }, [maskImage])

  useEffect(() => {
    if (!noticeKey && !copied) return
    const timeout = window.setTimeout(() => {
      setNoticeKey('')
      setCopied(false)
    }, 1800)
    return () => window.clearTimeout(timeout)
  }, [copied, noticeKey])

  const requestSettings: ImagePlazaSettings = { model, prompt: prompt.trim(), count, size, quality }

  const submit = async () => {
    setErrorKey('')
    setFileErrorKey('')
    if (!model) {
      setErrorKey('modelRequired')
      return
    }
    if (!requestSettings.prompt) {
      setErrorKey('promptRequired')
      return
    }
    if (mode === 'edit' && !originalImage) {
      setFileErrorKey('originalRequired')
      return
    }

    setBusy(true)
    try {
      const generated = mode === 'generate'
        ? await generateImages(requestSettings)
        : await editImage(requestSettings, originalImage!, maskImage)
      const createdAt = Date.now()
      const nextResults = generated.map((item): PlazaImage => ({
        ...item,
        model,
        prompt: requestSettings.prompt,
        createdAt,
      }))
      setResults((current) => [...nextResults, ...current].slice(0, 30))
      setSelectedId(nextResults[0]?.id)
      setNoticeKey('savingHistory')
      setHistorySaving(true)
      setSettingsOpen(false)
      void Promise.allSettled(nextResults.map(async (image) => ({
        localId: image.id,
        history: await saveImageHistory(image, image.model, image.prompt),
      }))).then((saved) => {
        const savedByLocalId = new Map<string, ImagePlazaHistoryRecord>()
        let failed = false
        for (const result of saved) {
          if (result.status === 'fulfilled') savedByLocalId.set(result.value.localId, result.value.history)
          else failed = true
        }
        setResults((current) => current.map((image) => {
          const history = savedByLocalId.get(image.id)
          return history ? {
            ...image,
            historyId: history.id,
            historyLoaded: true,
            requestId: history.request_id ?? image.requestId,
            format: history.format,
          } : image
        }))
        setHistorySaving(false)
        setNoticeKey(failed ? 'historySaveFailed' : 'generated')
      })
    } catch (error) {
      setErrorKey(error instanceof ImagePlazaRequestError ? error.kind : 'request_failed')
    } finally {
      setBusy(false)
    }
  }

  const continueEditing = async (image: PlazaImage) => {
    setErrorKey('')
    try {
      let blob = image.blob
      if (!blob && image.historyId) {
        blob = await loadImageHistoryImage(image.historyId, false)
      } else if (!blob) {
        const response = await fetch(image.src)
        if (!response.ok) throw new Error('image_unavailable')
        blob = await response.blob()
      }
      const type = blob.type.startsWith('image/') ? blob.type : 'image/png'
      setOriginalImage(new File([blob], `image-${image.id.slice(0, 8)}.png`, { type }))
      setMaskImage(undefined)
      setPrompt(image.prompt)
      setMode('edit')
      setFileErrorKey('')
      setSettingsOpen(window.matchMedia('(max-width: 1279px)').matches)
    } catch {
      setErrorKey('continueFailed')
    }
  }

  const copyPrompt = async () => {
    if (!selectedImage) return
    try {
      await navigator.clipboard.writeText(selectedImage.prompt)
      setCopied(true)
      setNoticeKey('')
    } catch {
      setCopied(false)
      setNoticeKey('copyFailed')
    }
  }

  const downloadImage = async () => {
    if (!selectedImage) return
    const extension = imageFileExtension(selectedImage.blob?.type ?? imageFormatMime(selectedImage.format))
    const fileName = `anyflows-image-${selectedImage.id.slice(0, 8)}.${extension}`
    if (selectedImage.blob) {
      triggerDownload(selectedImage.src, fileName)
      setNoticeKey('downloaded')
      return
    }
    try {
      const blob = selectedImage.historyId
        ? await loadImageHistoryImage(selectedImage.historyId, false)
        : await (async () => {
          const response = await fetch(selectedImage.src)
          if (!response.ok) throw new Error('image_unavailable')
          return response.blob()
        })()
      const url = URL.createObjectURL(blob)
      triggerDownload(url, `anyflows-image-${selectedImage.id.slice(0, 8)}.${imageFileExtension(blob.type)}`)
      window.setTimeout(() => URL.revokeObjectURL(url), 30_000)
      setNoticeKey('downloaded')
    } catch {
      setNoticeKey('downloadFailed')
    }
  }

  const clearHistory = async () => {
    if (!results.length || !window.confirm(t('imagePlaza.history.confirmClear'))) return
    try {
      await clearImageHistory()
      setResults([])
      setSelectedId(undefined)
      setNoticeKey('historyCleared')
    } catch {
      setNoticeKey('historyClearFailed')
    }
  }

  const deleteSelectedHistory = async () => {
    if (!selectedImage?.historyId || !window.confirm(t('imagePlaza.history.confirmDelete'))) return
    try {
      await deleteImageHistory(selectedImage.historyId)
      setResults((current) => current.filter((image) => image.id !== selectedImage.id))
      setSelectedId(undefined)
      setNoticeKey('imageDeleted')
    } catch {
      setNoticeKey('imageDeleteFailed')
    }
  }

  const onOriginalFile = (file?: File) => {
    if (file && !file.type.startsWith('image/')) {
      setFileErrorKey('invalidImage')
      return
    }
    setFileErrorKey('')
    setOriginalImage(file)
  }

  const onMaskFile = (file?: File) => {
    if (file && file.type !== 'image/png') {
      setFileErrorKey('maskPngOnly')
      return
    }
    setFileErrorKey('')
    setMaskImage(file)
  }

  const settingsProps: SettingsPanelProps = {
    mode,
    model,
    models,
    prompt,
    size,
    quality,
    count,
    busy,
    sourcePreview,
    maskPreview,
    originalImage,
    maskImage,
    fileErrorKey,
    errorKey,
    catalogQuery,
    onModel: setModel,
    onPrompt: setPrompt,
    onSize: setSize,
    onQuality: setQuality,
    onCount: setCount,
    onOriginal: onOriginalFile,
    onMask: onMaskFile,
    onClearOriginal: () => setOriginalImage(undefined),
    onClearMask: () => setMaskImage(undefined),
    onSubmit: () => void submit(),
  }

  const detailPanel = selectedImage ? (
    <div className="grid gap-4">
      <div className="overflow-hidden rounded-md border border-[var(--hairline)] bg-surface-2/50">
        <HistoryImage image={selectedImage} thumbnail={false} alt={selectedImage.prompt} className="aspect-square w-full object-contain" />
      </div>
      <div className="grid gap-2">
        <div className="flex items-center justify-between gap-2 text-xs">
          <span className="truncate font-medium" title={selectedImage.model}>{selectedImage.model}</span>
          <time className="shrink-0 text-muted-foreground" dateTime={new Date(selectedImage.createdAt).toISOString()}>
            {new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' }).format(selectedImage.createdAt)}
          </time>
        </div>
        <p className="max-h-28 overflow-y-auto whitespace-pre-wrap break-words text-xs leading-5 text-muted-foreground">{selectedImage.prompt}</p>
      </div>
      {selectedImage.requestId ? <code className="text-[0.6875rem] text-muted-foreground">{t('imagePlaza.detail.requestId', { id: selectedImage.requestId })}</code> : null}
      <div className="flex flex-wrap gap-2 border-t border-[var(--hairline)] pt-3">
        <Button size="sm" variant="bordered" className="rounded-md" onPress={() => void downloadImage()}>
          <Download className="size-3.5" aria-hidden="true" />{t('imagePlaza.actions.download')}
        </Button>
        <Button size="sm" variant="bordered" className="rounded-md" onPress={() => void copyPrompt()}>
          {copied ? <Check className="size-3.5" aria-hidden="true" /> : <Copy className="size-3.5" aria-hidden="true" />}
          {copied ? t('imagePlaza.actions.copied') : t('imagePlaza.actions.copyPrompt')}
        </Button>
        <Button size="sm" color="primary" variant="flat" className="rounded-md" onPress={() => void continueEditing(selectedImage)}>
          <Pencil className="size-3.5" aria-hidden="true" />{t('imagePlaza.actions.continueEdit')}
        </Button>
        {selectedImage.historyId ? (
          <Button isIconOnly size="sm" variant="light" aria-label={t('imagePlaza.actions.deleteImage')} title={t('imagePlaza.actions.deleteImage')} onPress={() => void deleteSelectedHistory()}>
            <Trash2 className="size-3.5" aria-hidden="true" />
          </Button>
        ) : null}
      </div>
    </div>
  ) : (
    <div className="grid min-h-48 place-items-center rounded-md border border-dashed border-[var(--hairline)] p-5 text-center text-xs leading-5 text-muted-foreground">
      {t('imagePlaza.detail.empty')}
    </div>
  )

  const requestError = errorKey && !['modelRequired', 'promptRequired'].includes(errorKey)

  return (
    <div className="relative min-h-[calc(100dvh-6rem)] min-w-0 pb-20 xl:pb-0">
      <header className="flex flex-col gap-4 border-b border-[var(--hairline)] pb-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-xl font-semibold">{t('imagePlaza.title')}</h1>
          <p className="mt-1 text-sm text-muted-foreground">{t('imagePlaza.subtitle')}</p>
        </div>
        <div role="group" aria-label={t('imagePlaza.mode.label')} className="inline-grid w-full grid-cols-2 rounded-md border border-[var(--hairline)] bg-surface-2/60 p-1 sm:w-auto">
          {(['generate', 'edit'] as const).map((item) => (
            <button
              key={item}
              type="button"
              aria-pressed={mode === item}
              className={cn(
                'flex h-9 items-center justify-center gap-2 rounded px-3 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60',
                mode === item ? 'bg-surface-1 text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground',
              )}
              onClick={() => { setMode(item); setErrorKey(''); setFileErrorKey('') }}
            >
              {item === 'generate' ? <Sparkles className="size-3.5" aria-hidden="true" /> : <Pencil className="size-3.5" aria-hidden="true" />}
              {t(`imagePlaza.mode.${item}`)}
            </button>
          ))}
        </div>
      </header>

      <div className="grid min-w-0 gap-5 pt-5 xl:grid-cols-[17rem_minmax(0,1fr)_19rem]">
        <aside className="hidden self-start border-r border-[var(--hairline)] pr-4 xl:block">
          <SettingsPanel {...settingsProps} instance="desktop" />
        </aside>

        <main className="min-w-0">
          <div className="mb-3 flex items-center justify-between gap-3">
            <div>
              <h2 className="text-sm font-semibold">{t('imagePlaza.gallery.title')}</h2>
              <p className="mt-1 text-xs text-muted-foreground">{t('imagePlaza.gallery.historyCount', { count: results.length })}</p>
              {noticeKey && !historySaving ? <p role="status" className="mt-1 text-xs text-muted-foreground">{t(`imagePlaza.status.${noticeKey}`)}</p> : null}
            </div>
            <div className="flex items-center gap-2">
              {busy || historySaving ? (
                <span role="status" className="flex items-center gap-2 text-xs text-muted-foreground">
                  <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />{t(busy ? 'imagePlaza.actions.submitting' : 'imagePlaza.status.savingHistory')}
                </span>
              ) : null}
              {results.length ? (
                <Button
                  isIconOnly
                  aria-label={t('imagePlaza.actions.clearHistory')}
                  title={t('imagePlaza.actions.clearHistory')}
                  size="sm"
                  variant="light"
                  isDisabled={busy || historySaving}
                  onPress={() => void clearHistory()}
                >
                  <Trash2 className="size-4" aria-hidden="true" />
                </Button>
              ) : null}
            </div>
          </div>

          {results.length ? (
            <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 2xl:grid-cols-4">
              <AnimatePresence initial={false}>
                {results.map((image) => (
                  <motion.button
                    key={image.id}
                    type="button"
                    initial={prefersReducedMotion ? false : { opacity: 0, y: 6 }}
                    animate={{ opacity: 1, y: 0 }}
                    exit={prefersReducedMotion ? undefined : { opacity: 0 }}
                    transition={{ duration: prefersReducedMotion ? 0 : 0.16 }}
                    aria-label={t('imagePlaza.gallery.select', { prompt: image.prompt })}
                    aria-pressed={selectedImage?.id === image.id}
                    className={cn(
                      'group aspect-square overflow-hidden rounded-md border bg-surface-2/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/70',
                      selectedImage?.id === image.id ? 'border-primary ring-1 ring-primary/60' : 'border-[var(--hairline)] hover:border-foreground/35',
                    )}
                    onClick={() => setSelectedId(image.id)}
                  >
                    <HistoryImage image={image} thumbnail alt="" className="size-full object-cover transition-transform duration-200 group-hover:scale-[1.02] motion-reduce:transition-none" />
                  </motion.button>
                ))}
              </AnimatePresence>
            </div>
          ) : (
            <div className="grid min-h-[22rem] place-items-center border border-dashed border-[var(--hairline)] px-5 py-10 text-center">
              <div className="grid justify-items-center">
                <ImageIcon className="size-8 text-muted-foreground" aria-hidden="true" />
                <h3 className="mt-4 text-sm font-semibold">{t('imagePlaza.gallery.emptyTitle')}</h3>
                <p className="mt-2 max-w-sm text-xs leading-5 text-muted-foreground">{t('imagePlaza.gallery.emptyBody')}</p>
              </div>
            </div>
          )}
          {requestError ? (
            <p role="alert" className="mt-3 flex items-center gap-2 text-xs text-danger">
              <AlertCircle className="size-4" aria-hidden="true" />{t(`imagePlaza.errors.${errorKey}`)}
            </p>
          ) : null}
        </main>

        <aside className="hidden self-start border-l border-[var(--hairline)] pl-4 xl:block">
          <h2 className="mb-3 text-sm font-semibold">{t('imagePlaza.detail.title')}</h2>
          {detailPanel}
        </aside>

        <section className="min-w-0 border-t border-[var(--hairline)] pt-4 xl:hidden" aria-label={t('imagePlaza.detail.title')}>
          <h2 className="mb-3 text-sm font-semibold">{t('imagePlaza.detail.title')}</h2>
          {detailPanel}
        </section>
      </div>

      <Button
        className="fixed bottom-[calc(env(safe-area-inset-bottom)+1rem)] left-1/2 z-30 -translate-x-1/2 rounded-full shadow-lg xl:hidden"
        color="primary"
        onPress={() => setSettingsOpen(true)}
      >
        <SlidersHorizontal className="size-4" aria-hidden="true" />{t('imagePlaza.actions.settings')}
      </Button>

      <Drawer
        backdrop="blur"
        classNames={{ base: 'max-h-[88dvh] rounded-t-xl' }}
        isOpen={settingsOpen}
        placement="bottom"
        scrollBehavior="inside"
        onOpenChange={setSettingsOpen}
      >
        <DrawerContent>
          {() => (
            <>
              <DrawerHeader className="border-b border-[var(--hairline)] px-5 py-4">
                <div className="flex w-full items-center justify-between">
                  <div>
                    <h2 className="text-base font-semibold">{t('imagePlaza.controls.title')}</h2>
                    <p className="mt-1 text-xs text-muted-foreground">{t(`imagePlaza.mode.${mode}`)}</p>
                  </div>
                  <Button isIconOnly aria-label={t('imagePlaza.actions.close')} size="sm" variant="light" onPress={() => setSettingsOpen(false)}>
                    <X className="size-4" aria-hidden="true" />
                  </Button>
                </div>
              </DrawerHeader>
              <DrawerBody className="gap-0 p-0"><SettingsPanel {...settingsProps} instance="mobile" /></DrawerBody>
            </>
          )}
        </DrawerContent>
      </Drawer>
    </div>
  )
}

type SettingsPanelProps = {
  mode: ImageMode
  model: string
  models: ModelCatalogItem[]
  prompt: string
  size: string
  quality: string
  count: number
  busy: boolean
  sourcePreview: string
  maskPreview: string
  originalImage?: File
  maskImage?: File
  fileErrorKey: string
  errorKey: string
  catalogQuery: ImageCatalogQuery
  instance?: 'desktop' | 'mobile'
  onModel: (value: string) => void
  onPrompt: (value: string) => void
  onSize: (value: string) => void
  onQuality: (value: string) => void
  onCount: (value: number) => void
  onOriginal: (file?: File) => void
  onMask: (file?: File) => void
  onClearOriginal: () => void
  onClearMask: () => void
  onSubmit: () => void
}

function SettingsPanel(props: SettingsPanelProps) {
  const { t } = useTranslation()
  const instance = props.instance ?? 'desktop'
  const canShowError = ['modelRequired', 'promptRequired'].includes(props.errorKey)
  const modelId = `image-plaza-model-${instance}`
  const promptId = `image-plaza-prompt-${instance}`
  const sizeId = `image-plaza-size-${instance}`
  const qualityId = `image-plaza-quality-${instance}`
  const countId = `image-plaza-count-${instance}`

  return (
    <div className="grid gap-5 p-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h2 className="text-sm font-semibold">{t('imagePlaza.controls.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('imagePlaza.controls.subtitle')}</p>
        </div>
        <Button
          isIconOnly
          aria-label={t('imagePlaza.actions.refreshModels')}
          className="size-8 min-w-8"
          isDisabled={props.catalogQuery.isFetching}
          size="sm"
          title={t('imagePlaza.actions.refreshModels')}
          variant="light"
          onPress={() => void props.catalogQuery.refetch()}
        >
          <RefreshCw className={cn('size-4', props.catalogQuery.isFetching && 'animate-spin')} aria-hidden="true" />
        </Button>
      </div>

      <div className="grid gap-2">
        <label className="text-xs font-medium" htmlFor={modelId}>{t('imagePlaza.controls.model')}</label>
        <Select
          id={modelId}
          aria-label={t('imagePlaza.controls.model')}
          classNames={{ trigger: 'min-h-10 rounded-md bg-surface-2/70', value: 'text-sm' }}
          isDisabled={props.catalogQuery.isPending || props.models.length === 0}
          placeholder={props.catalogQuery.isPending ? t('imagePlaza.models.loading') : t('imagePlaza.models.empty')}
          selectedKeys={props.model ? [props.model] : []}
          size="sm"
          onSelectionChange={(keys) => props.onModel(String(Array.from(keys)[0] ?? ''))}
        >
          {props.models.map((item) => <SelectItem key={item.model}>{item.model}</SelectItem>)}
        </Select>
        <div className="flex flex-wrap items-center justify-between gap-2 text-[0.6875rem] text-muted-foreground">
          <span>{props.catalogQuery.isError ? t('imagePlaza.models.error') : t('imagePlaza.models.count', { count: props.models.length })}</span>
          {props.catalogQuery.hasNextPage ? (
            <button
              type="button"
              className="font-medium text-primary hover:underline disabled:opacity-50"
              disabled={props.catalogQuery.isFetchingNextPage}
              onClick={() => void props.catalogQuery.fetchNextPage()}
            >
              {props.catalogQuery.isFetchingNextPage ? t('imagePlaza.models.loading') : t('imagePlaza.models.loadMore')}
            </button>
          ) : null}
        </div>
        {props.catalogQuery.isError ? (
          <Button size="sm" variant="bordered" onPress={() => void props.catalogQuery.refetch()}>
            <RefreshCw className="size-3.5" aria-hidden="true" />{t('imagePlaza.actions.retry')}
          </Button>
        ) : null}
      </div>

      <div className="grid gap-2">
        <label className="text-xs font-medium" htmlFor={promptId}>{t('imagePlaza.controls.prompt')}</label>
        <Textarea
          id={promptId}
          aria-label={t('imagePlaza.controls.prompt')}
          minRows={5}
          maxRows={9}
          placeholder={t(props.mode === 'generate' ? 'imagePlaza.controls.promptPlaceholder' : 'imagePlaza.controls.editPromptPlaceholder')}
          value={props.prompt}
          onValueChange={props.onPrompt}
          classNames={{ inputWrapper: 'rounded-md bg-surface-2/70 shadow-none', input: 'text-sm leading-5' }}
        />
      </div>

      {props.mode === 'edit' ? (
        <div className="grid gap-3 border-t border-[var(--hairline)] pt-4">
          <FilePicker
            accept="image/png,image/jpeg,image/webp"
            label={t('imagePlaza.controls.original')}
            preview={props.sourcePreview}
            fileName={props.originalImage?.name}
            onChange={props.onOriginal}
            onClear={props.onClearOriginal}
          />
          <FilePicker
            accept="image/png"
            label={t('imagePlaza.controls.mask')}
            hint={t('imagePlaza.controls.maskHint')}
            preview={props.maskPreview}
            fileName={props.maskImage?.name}
            onChange={props.onMask}
            onClear={props.onClearMask}
          />
          {props.fileErrorKey ? <p className="text-xs text-danger">{t(`imagePlaza.errors.${props.fileErrorKey}`)}</p> : null}
        </div>
      ) : null}

      <div className="grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-4">
        <label className="grid gap-2 text-xs font-medium" htmlFor={sizeId}>
          {t('imagePlaza.controls.size')}
          <select id={sizeId} className="h-9 min-w-0 rounded-md border border-[var(--hairline)] bg-surface-2/70 px-2 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/60" value={props.size} onChange={(event) => props.onSize(event.target.value)}>
            {imageSizes.map((item) => <option key={item} value={item}>{item}</option>)}
          </select>
        </label>
        <label className="grid gap-2 text-xs font-medium" htmlFor={qualityId}>
          {t('imagePlaza.controls.quality')}
          <select id={qualityId} className="h-9 min-w-0 rounded-md border border-[var(--hairline)] bg-surface-2/70 px-2 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/60" value={props.quality} onChange={(event) => props.onQuality(event.target.value)}>
            {imageQualities.map((item) => <option key={item} value={item}>{t(`imagePlaza.quality.${item}`)}</option>)}
          </select>
        </label>
      </div>

      <label className="grid gap-2 text-xs font-medium" htmlFor={countId}>
        <span className="flex items-center justify-between">
          {t('imagePlaza.controls.count')}
          <span className="font-mono text-muted-foreground">{props.count}</span>
        </span>
        <input id={countId} type="range" min={1} max={4} step={1} value={props.count} className="h-2 w-full cursor-pointer accent-primary" onChange={(event) => props.onCount(Number(event.target.value))} />
      </label>

      {canShowError ? (
        <div role="alert" className="flex items-start gap-2 rounded-md border border-danger/25 bg-danger/5 p-2.5 text-xs leading-5 text-danger">
          <AlertCircle className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <span>{t(`imagePlaza.errors.${props.errorKey}`)}</span>
        </div>
      ) : null}

      <Button color="primary" isDisabled={props.busy || !props.models.length} isLoading={props.busy} className="h-10 rounded-md font-medium" onPress={props.onSubmit}>
        {props.mode === 'generate' ? <Sparkles className="size-4" aria-hidden="true" /> : <Pencil className="size-4" aria-hidden="true" />}
        {props.busy ? t('imagePlaza.actions.submitting') : t(props.mode === 'generate' ? 'imagePlaza.actions.generate' : 'imagePlaza.actions.edit')}
      </Button>
    </div>
  )
}

function FilePicker(props: {
  accept: string
  fileName?: string
  hint?: string
  label: string
  onChange: (file?: File) => void
  onClear: () => void
  preview: string
}) {
  const { t } = useTranslation()
  return (
    <div className="grid gap-2">
      <span className="text-xs font-medium">{props.label}</span>
      <label className="flex min-h-10 cursor-pointer items-center gap-2 rounded-md border border-dashed border-[var(--hairline)] px-3 py-2 text-xs text-muted-foreground transition-colors hover:border-foreground/35 hover:text-foreground">
        {props.preview ? <img src={props.preview} alt="" className="size-9 rounded object-cover" /> : <ImagePlus className="size-4 shrink-0" aria-hidden="true" />}
        <span className="min-w-0 flex-1 truncate">{props.fileName || t('imagePlaza.controls.chooseFile')}</span>
        <input
          type="file"
          accept={props.accept}
          className="sr-only"
          onChange={(event) => {
            props.onChange(event.target.files?.[0])
            event.currentTarget.value = ''
          }}
        />
      </label>
      {props.hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{props.hint}</p> : null}
      {props.fileName ? (
        <Button size="sm" variant="light" className="h-7 min-h-7 justify-start px-1 text-xs text-muted-foreground" onPress={props.onClear}>
          <X className="size-3.5" aria-hidden="true" />{t('imagePlaza.actions.removeFile')}
        </Button>
      ) : null}
    </div>
  )
}

function triggerDownload(url: string, fileName: string) {
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = fileName
  anchor.rel = 'noreferrer'
  document.body.append(anchor)
  anchor.click()
  anchor.remove()
}

function historyImage(entry: ImagePlazaHistoryRecord): PlazaImage {
  return {
    id: entry.id,
    src: '',
    model: entry.model,
    prompt: entry.prompt,
    revisedPrompt: entry.revised_prompt ?? undefined,
    requestId: entry.request_id ?? undefined,
    createdAt: entry.created_at * 1000,
    historyId: entry.id,
    historyLoaded: true,
    format: entry.format,
  }
}

type HistoryImageProps = {
  image: PlazaImage
  thumbnail?: boolean
  alt: string
  className: string
}

function HistoryImage({ image, thumbnail = false, alt, className }: HistoryImageProps) {
  const imageRef = useRef<HTMLImageElement>(null)
  const [source, setSource] = useState(image.historyLoaded ? '' : image.src)

  useEffect(() => {
    if (!image.historyLoaded || !image.historyId || image.blob) {
      setSource(image.src)
      return
    }

    const element = imageRef.current
    if (!element) return
    let active = true
    let objectUrl = ''
    let observer: IntersectionObserver | undefined
    const load = () => {
      void loadImageHistoryImage(image.historyId!, thumbnail).then((blob) => {
        if (!active) return
        objectUrl = URL.createObjectURL(blob)
        setSource(objectUrl)
      }).catch(() => undefined)
    }
    if ('IntersectionObserver' in window) {
      observer = new IntersectionObserver((entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          observer?.disconnect()
          load()
        }
      })
      observer.observe(element)
    } else {
      load()
    }
    return () => {
      active = false
      observer?.disconnect()
      if (objectUrl) URL.revokeObjectURL(objectUrl)
    }
  }, [image.historyId, image.historyLoaded, image.src, thumbnail])

  return <img ref={imageRef} src={source || undefined} alt={alt} loading="lazy" referrerPolicy="no-referrer" className={className} />
}

function imageFormatMime(format?: ImagePlazaHistoryRecord['format']) {
  if (format === 'jpeg') return 'image/jpeg'
  if (format === 'webp') return 'image/webp'
  return 'image/png'
}

function imageFileExtension(mimeType?: string) {
  switch (mimeType) {
    case 'image/jpeg': return 'jpg'
    case 'image/webp': return 'webp'
    default: return 'png'
  }
}
