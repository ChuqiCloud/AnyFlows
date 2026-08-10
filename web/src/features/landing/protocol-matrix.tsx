import { useTranslation } from 'react-i18next'

/**
 * 三类协议族既可作客户端入口也可作上游出口，因此两侧共用同一份清单。
 * 顺序固定，避免每次渲染视觉跳动。
 */
const protocols = [
  { id: 'openai', label: 'OpenAI', detail: 'Chat / Responses / Embeddings / Images / Audio' },
  { id: 'anthropic', label: 'Anthropic', detail: 'Messages' },
  { id: 'gemini', label: 'Gemini', detail: 'generateContent' },
] as const

/**
 * 协议矩阵示意：任一协议进 → Canonical IR → 任一协议出。
 *
 * 刻意不画 N×M 的九条连线——那样只会得到一团网。收敛到中间节点再扇出，
 * 恰好也是真实架构的形状：正是中间表示让 N×M 不必逐对实现。
 */
export function ProtocolMatrix() {
  const { t } = useTranslation()

  return (
    <div className="mt-16 rounded-xl border border-[var(--hairline)] bg-[var(--surface-sunken)] p-6 sm:p-10">
      <div className="grid items-center gap-8 lg:grid-cols-[1fr_auto_1fr] lg:gap-6">
        <ProtocolColumn label={t('landing.protocol.clientLabel')} align="start" />

        {/*
          枢纽的主轴必须随断点翻转：窄屏三列上下堆叠，引线是竖的；
          宽屏三列左右并排，引线要变横的。只改 Connector 的尺寸不够，
          容器方向不跟着换，横线会被继续竖排。
        */}
        <div className="flex flex-col items-center gap-3 lg:flex-row">
          <Connector />
          <div className="rounded-lg border border-[var(--hairline-strong)] bg-[var(--surface-raised)] px-5 py-3 text-center shadow-[var(--shadow-md)]">
            {/* 规范化协议名是可翻译标签，不是字标：走正文字体，
                否则中文会落到 Instrument Sans 的合成斜体上。 */}
            <span className="text-sm font-semibold tracking-normal whitespace-nowrap">
              {t('landing.protocol.canonical')}
            </span>
          </div>
          <Connector />
        </div>

        <ProtocolColumn label={t('landing.protocol.upstreamLabel')} align="end" />
      </div>

      <p className="mt-8 text-center text-xs text-muted-foreground">
        {t('landing.protocol.note')}
      </p>
    </div>
  )
}

type ProtocolColumnProps = {
  label: string
  /** 决定文本与列在宽屏下贴左还是贴右，形成向中心收敛的视觉 */
  align: 'start' | 'end'
}

function ProtocolColumn({ label, align }: ProtocolColumnProps) {
  return (
    <div>
      <p
        className={
          align === 'end'
            ? 'mb-4 text-center text-xs font-medium tracking-normal text-muted-foreground/70 uppercase lg:text-right'
            : 'mb-4 text-center text-xs font-medium tracking-normal text-muted-foreground/70 uppercase lg:text-left'
        }
      >
        {label}
      </p>
      <ul className="flex flex-col gap-2.5">
        {protocols.map((protocol) => (
          <li
            key={protocol.id}
            className="flex items-baseline justify-between gap-3 rounded-lg border border-[var(--hairline)] bg-[var(--surface-raised)] px-4 py-3"
          >
            <span className="text-sm font-medium">{protocol.label}</span>
            <span className="font-mono text-[0.6875rem] text-muted-foreground">
              {protocol.detail}
            </span>
          </li>
        ))}
      </ul>
    </div>
  )
}

/** 通向枢纽的引线：窄屏为竖向，宽屏转为横向 */
function Connector() {
  return (
    <span
      className="h-6 w-px bg-[var(--hairline-strong)] lg:h-px lg:w-10"
      aria-hidden="true"
    />
  )
}
