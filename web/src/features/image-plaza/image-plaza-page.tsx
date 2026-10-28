import {
  Download,
  ImageIcon,
  Images,
  Loader2,
  PanelRightOpen,
  Pencil,
  RefreshCw,
  Sparkles,
  Upload,
  X,
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Textarea } from '@/components/ui/textarea'
import { useIsMobile } from '@/hooks/use-mobile'
import { EMPTY_MODEL_CATALOG_FILTERS, useModelCatalog } from '@/features/models/model-api'
import type { ModelCatalogItem, ModelCatalogModality } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { editImage, generateImage, imageSource, type GeneratedImage } from './image-plaza-client'

type Mode = 'generate' | 'edit'
type GalleryItem = {
  id: string
  src: string
  model: string
  prompt: string
  created?: number
  revisedPrompt?: string
  options: { size: string; quality: string; responseFormat: string }
}

const sizes = ['1024x1024', '1536x1024', '1024x1536']
const qualities = ['auto', 'low', 'medium', 'high']

function toGalleryImages(data: GeneratedImage[] | undefined, model: string, prompt: string, options: GalleryItem['options'], created?: number): GalleryItem[] {
  return (data ?? []).flatMap((item, index) => {
    const src = imageSource(item)
    return src ? [{ id: `${created ?? Date.now()}-${index}-${Math.random()}`, src, model, prompt, created, revisedPrompt: item.revised_prompt, options }] : []
  })
}

async function fileFromSource(src: string) {
  const response = await fetch(src)
  const blob = await response.blob()
  return new File([blob], 'image-plaza-edit.png', { type: blob.type || 'image/png' })
}

export function ImagePlazaPage() {
  const { t } = useTranslation()
  const isMobile = useIsMobile()
  const imageFilters = useMemo(() => ({
    ...EMPTY_MODEL_CATALOG_FILTERS,
    outputModalities: ['image'] as ModelCatalogModality[],
  }), [])
  const catalogQuery = useModelCatalog('', imageFilters, true)
  const models = useMemo(() => catalogQuery.data?.pages.flatMap((page) => page.models) ?? [], [catalogQuery.data])
  const [mode, setMode] = useState<Mode>('generate')
  const [model, setModel] = useState('')
  const [prompt, setPrompt] = useState('')
  const [size, setSize] = useState(sizes[0])
  const [quality, setQuality] = useState(qualities[0])
  const [count, setCount] = useState('1')
  const [responseFormat, setResponseFormat] = useState<'url' | 'b64_json'>('url')
  const [sourceFile, setSourceFile] = useState<File>()
  const [maskFile, setMaskFile] = useState<File>()
  const [gallery, setGallery] = useState<GalleryItem[]>([])
  const [selected, setSelected] = useState<GalleryItem>()
  const [running, setRunning] = useState(false)
  const [error, setError] = useState('')
  const fileInput = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!model && models[0]) setModel(models[0].model)
  }, [model, models])

  const selectedModel: ModelCatalogItem | undefined = models.find((item) => item.model === model)
  const submit = useCallback(async () => {
    if (!model || !prompt.trim() || (mode === 'edit' && !sourceFile)) return
    setRunning(true)
    setError('')
    try {
      const options = { model, prompt: prompt.trim(), n: Math.min(4, Math.max(1, Number(count) || 1)), size, quality, responseFormat }
      const result = mode === 'edit' ? await editImage(sourceFile!, options, maskFile) : await generateImage(options)
      const next = toGalleryImages(result.data, model, prompt.trim(), { size, quality, responseFormat }, result.created)
      setGallery((current) => [...next, ...current])
      if (next[0]) setSelected(next[0])
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : t('imagePlaza.errors.request'))
    } finally {
      setRunning(false)
    }
  }, [count, maskFile, mode, model, prompt, quality, responseFormat, size, sourceFile, t])

  const continueEditing = async (item: GalleryItem) => {
    setSelected(item)
    setMode('edit')
    setPrompt(item.prompt)
    setMaskFile(undefined)
    try {
      setSourceFile(await fileFromSource(item.src))
    } catch {
      setError(t('imagePlaza.errors.loadImage'))
    }
  }

  const controls = (
    <div className="grid gap-4">
      <div className="flex rounded-lg border border-[var(--hairline)] bg-surface-2/50 p-1">
        {(['generate', 'edit'] as const).map((value) => <button key={value} type="button" className={cn('flex-1 rounded-md px-3 py-2 text-sm font-medium transition-colors', mode === value ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground')} onClick={() => setMode(value)}>{value === 'generate' ? <span className="inline-flex items-center gap-2"><Sparkles className="size-4" />{t('imagePlaza.modes.generate')}</span> : <span className="inline-flex items-center gap-2"><Pencil className="size-4" />{t('imagePlaza.modes.edit')}</span>}</button>)}
      </div>
      <label className="grid gap-1.5 text-sm"><span className="font-medium">{t('imagePlaza.model')}</span><Select value={model} onChange={(event) => setModel(event.target.value)} disabled={catalogQuery.isPending || models.length === 0}><option value="">{catalogQuery.isPending ? t('imagePlaza.loadingModels') : t('imagePlaza.noModels')}</option>{models.map((item) => <option key={item.model} value={item.model}>{item.display_name || item.model}</option>)}</Select>{selectedModel ? <span className="text-xs text-muted-foreground">{selectedModel.provider} · {selectedModel.model}</span> : null}</label>
      <label className="grid gap-1.5 text-sm"><span className="font-medium">{t('imagePlaza.prompt')}</span><Textarea value={prompt} onChange={(event) => setPrompt(event.target.value)} placeholder={t(mode === 'generate' ? 'imagePlaza.promptPlaceholder' : 'imagePlaza.editPlaceholder')} rows={5} /></label>
      {mode === 'edit' ? <div className="grid gap-2"><span className="text-sm font-medium">{t('imagePlaza.sourceImage')}</span><input ref={fileInput} type="file" accept="image/png,image/jpeg,image/webp" className="sr-only" onChange={(event) => setSourceFile(event.target.files?.[0])} /><Button type="button" variant="outline" className="justify-start" onClick={() => fileInput.current?.click()}><Upload className="size-4" />{sourceFile?.name ?? t('imagePlaza.chooseImage')}</Button>{sourceFile ? <div className="flex items-center gap-2 text-xs text-muted-foreground"><ImageIcon className="size-3.5" />{Math.ceil(sourceFile.size / 1024)} KB<Button type="button" size="icon-xs" variant="ghost" onClick={() => setSourceFile(undefined)} aria-label={t('imagePlaza.clearImage')}><X /></Button></div> : null}<label className="mt-2 grid gap-1.5 text-sm"><span className="font-medium">{t('imagePlaza.maskImage')}</span><input type="file" accept="image/png" onChange={(event) => setMaskFile(event.target.files?.[0])} /><span className="text-xs text-muted-foreground">{maskFile?.name ?? t('imagePlaza.maskHint')}</span></label></div> : null}
      <div className="grid grid-cols-2 gap-3"><label className="grid gap-1.5 text-sm"><span className="font-medium">{t('imagePlaza.size')}</span><Select value={size} onChange={(event) => setSize(event.target.value)}>{sizes.map((value) => <option key={value}>{value}</option>)}</Select></label><label className="grid gap-1.5 text-sm"><span className="font-medium">{t('imagePlaza.quality')}</span><Select value={quality} onChange={(event) => setQuality(event.target.value)}>{qualities.map((value) => <option key={value}>{value}</option>)}</Select></label></div>
      <div className="grid grid-cols-2 gap-3"><label className="grid gap-1.5 text-sm"><span className="font-medium">{t('imagePlaza.count')}</span><Input type="number" min={1} max={4} value={count} onChange={(event) => setCount(event.target.value)} /></label><label className="grid gap-1.5 text-sm"><span className="font-medium">{t('imagePlaza.responseFormat')}</span><Select value={responseFormat} onChange={(event) => setResponseFormat(event.target.value as 'url' | 'b64_json')}><option value="url">URL</option><option value="b64_json">Base64</option></Select></label></div>
      {error ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p> : null}
      <Button type="button" variant="brand" size="lg" disabled={running || !model || !prompt.trim() || (mode === 'edit' && !sourceFile)} onClick={() => void submit()}>{running ? <Loader2 className="animate-spin" /> : <Sparkles />}{running ? t('imagePlaza.generating') : mode === 'generate' ? t('imagePlaza.generate') : t('imagePlaza.edit')}</Button>
    </div>
  )

  const details = selected ? <div className="grid gap-4"><div className="overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-2"><img src={selected.src} alt={selected.prompt} className="max-h-[28rem] w-full object-contain" /></div><div><h3 className="font-semibold">{t('imagePlaza.details')}</h3><p className="mt-1 text-sm text-muted-foreground">{selected.prompt}</p>{selected.revisedPrompt ? <p className="mt-2 text-xs text-muted-foreground">{t('imagePlaza.revisedPrompt')}: {selected.revisedPrompt}</p> : null}</div><dl className="grid grid-cols-2 gap-2 text-xs"><div><dt className="text-muted-foreground">{t('imagePlaza.model')}</dt><dd className="font-mono">{selected.model}</dd></div><div><dt className="text-muted-foreground">{t('imagePlaza.size')}</dt><dd>{selected.options.size}</dd></div><div><dt className="text-muted-foreground">{t('imagePlaza.quality')}</dt><dd>{selected.options.quality}</dd></div><div><dt className="text-muted-foreground">{t('imagePlaza.responseFormat')}</dt><dd>{selected.options.responseFormat}</dd></div></dl><div className="flex flex-wrap gap-2"><Button asChild variant="secondary"><a href={selected.src} download="anyflows-image.png"><Download />{t('imagePlaza.download')}</a></Button><Button type="button" variant="outline" onClick={() => void continueEditing(selected)}><Pencil />{t('imagePlaza.continueEditing')}</Button></div></div> : <div className="grid min-h-48 place-items-center rounded-xl border border-dashed border-[var(--hairline)] text-center text-sm text-muted-foreground"><Images className="mb-2 size-8 opacity-40" /><p>{t('imagePlaza.selectResult')}</p></div>

  return <div className="flex min-h-[calc(100dvh-5.75rem)] min-w-0 flex-col gap-5"><header className="flex flex-wrap items-end justify-between gap-3"><div><p className="text-xs font-semibold tracking-[0.18em] text-brand uppercase">{t('imagePlaza.eyebrow')}</p><h1 className="mt-1 text-2xl font-semibold tracking-tight">{t('imagePlaza.title')}</h1><p className="mt-1 max-w-2xl text-sm text-muted-foreground">{t('imagePlaza.subtitle')}</p></div>{selected && isMobile ? <Sheet open={Boolean(selected)} onOpenChange={(open) => !open && setSelected(undefined)}><Button type="button" variant="outline" onClick={() => setSelected(selected)}><PanelRightOpen />{t('imagePlaza.openDetails')}</Button><SheetContent side="bottom" className="max-h-[90dvh] overflow-y-auto"><SheetHeader><SheetTitle>{t('imagePlaza.details')}</SheetTitle><SheetDescription>{selected.prompt}</SheetDescription></SheetHeader><div className="px-4 pb-6">{details}</div></SheetContent></Sheet> : null}</header><div className="grid min-h-0 gap-5 lg:grid-cols-[minmax(18rem,22rem)_minmax(0,1fr)]"><Card><CardHeader><CardTitle>{t('imagePlaza.controlsTitle')}</CardTitle><CardDescription>{t('imagePlaza.controlsDescription')}</CardDescription></CardHeader><CardContent>{controls}</CardContent></Card><section className="min-w-0"><div className="mb-3 flex items-center justify-between"><div><h2 className="font-semibold">{t('imagePlaza.gallery')}</h2><p className="text-xs text-muted-foreground">{t('imagePlaza.galleryHint')}</p></div><Button type="button" size="icon-sm" variant="ghost" onClick={() => void catalogQuery.refetch()} title={t('imagePlaza.refreshModels')}><RefreshCw className={cn(catalogQuery.isFetching && 'animate-spin')} /></Button></div>{gallery.length === 0 ? <div className="grid min-h-72 place-items-center rounded-xl border border-dashed border-[var(--hairline)] text-center text-sm text-muted-foreground"><Images className="mb-2 size-10 opacity-30" /><p>{t('imagePlaza.empty')}</p></div> : <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4">{gallery.map((item) => <button type="button" key={item.id} className={cn('group overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1 text-left transition hover:border-brand/60', selected?.id === item.id && 'border-brand ring-2 ring-brand/20')} onClick={() => setSelected(item)}><img src={item.src} alt={item.prompt} loading="lazy" className="aspect-square w-full object-cover transition duration-300 group-hover:scale-[1.02]" /><span className="block truncate px-2.5 py-2 text-xs text-muted-foreground">{item.model}</span></button>)}</div>}</section></div>{!isMobile ? <aside className="grid gap-3 lg:max-w-3xl"><h2 className="flex items-center gap-2 font-semibold"><PanelRightOpen className="size-4 text-brand" />{t('imagePlaza.details')}</h2>{details}</aside> : null}</div>
}
