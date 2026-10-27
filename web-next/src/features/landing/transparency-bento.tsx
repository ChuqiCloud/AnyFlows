import { CheckCircle2, FileText, Layers, ShieldCheck } from 'lucide-react'

export function TransparencyBento() {
  const upstreams = [
    { name: 'Anthropic Claude API', status: 'Operational', uptime: '99.98%', latency: '82ms' },
    { name: 'OpenAI API & Responses', status: 'Operational', uptime: '99.99%', latency: '74ms' },
    { name: 'DeepSeek Platform', status: 'Operational', uptime: '99.95%', latency: '92ms' },
    { name: 'Google Gemini API', status: 'Operational', uptime: '99.99%', latency: '58ms' },
  ]

  const recentLogs = [
    { time: '13:08:42', model: 'claude-opus-5-5', tokens: '1,248 / 386', latency: '82ms', cost: '¥0.0058', status: '200' },
    { time: '13:08:15', model: 'deepseek-flash', tokens: '2,410 / 1,024', latency: '92ms', cost: '¥0.0024', status: '200' },
    { time: '13:07:50', model: 'gpt-6.1-sol', tokens: '890 / 240', latency: '74ms', cost: '¥0.0032', status: '200' },
    { time: '13:07:11', model: 'gemini-3.8-flash', tokens: '3,100 / 512', latency: '58ms', cost: '¥0.0003', status: '200' },
  ]

  return (
    <section className="mx-auto w-full max-w-[1240px] px-4 py-20 sm:px-6 sm:py-24 lg:px-8">
      {/* Header */}
      <div className="max-w-2xl">
        <div className="inline-flex items-center gap-1.5 font-mono text-xs font-semibold uppercase tracking-[0.2em] text-primary">
          <ShieldCheck className="size-3.5" />
          <span>TRANSPARENCY & OBSERVABILITY · 状态与用量</span>
        </div>
        <h2 className="mt-3 text-3xl font-bold tracking-tight text-foreground sm:text-4xl">
          模型表现、官方状态与消费明细
        </h2>
        <p className="mt-2.5 text-base leading-7 text-muted-foreground">
          拒绝黑盒中转。每一次调用、每一笔费用、每一个通道的健康状态全部公开透明，随时可审计回溯。
        </p>
      </div>

      {/* 3 Bento Cards */}
      <div className="mt-12 grid gap-6 lg:grid-cols-3">
        {/* Card 1: Official Service Status */}
        <div className="rounded-2xl border border-[var(--hairline)] bg-surface-1 p-6 shadow-xs flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between">
              <span className="font-mono text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                服务状态监控
              </span>
              <span className="inline-flex items-center gap-1.5 rounded-full bg-success/15 px-2.5 py-0.5 text-xs font-semibold text-success">
                <span className="size-1.5 rounded-full bg-success animate-pulse" />
                全线正常
              </span>
            </div>
            <h3 className="mt-4 text-lg font-semibold text-foreground">
              上游服务可用率与心跳探活
            </h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              持续多节点心跳探测，遇 429 限流或 5xx 故障自动无感降级转移至候选渠道。
            </p>

            <div className="mt-6 space-y-2.5">
              {upstreams.map((item) => (
                <div
                  key={item.name}
                  className="flex items-center justify-between rounded-xl border border-[var(--hairline)] bg-surface-sunken p-2.5 text-xs"
                >
                  <div className="flex items-center gap-2">
                    <CheckCircle2 className="size-3.5 text-success shrink-0" />
                    <span className="font-medium text-foreground">{item.name}</span>
                  </div>
                  <div className="flex items-center gap-2.5 font-mono text-[0.6875rem]">
                    <span className="text-success">{item.latency}</span>
                    <span className="text-muted-foreground">{item.uptime}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div className="mt-6 pt-4 border-t border-[var(--hairline)] flex items-center justify-between text-xs text-muted-foreground">
            <span>SLA 保证 99.98%</span>
            <span className="font-mono text-[0.6875rem]">自动容灾秒级切换</span>
          </div>
        </div>

        {/* Card 2: Granular Request Traces & Billing */}
        <div className="rounded-2xl border border-[var(--hairline)] bg-surface-1 p-6 shadow-xs flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between">
              <span className="font-mono text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                调用审计与扣费
              </span>
              <FileText className="size-4 text-muted-foreground" />
            </div>
            <h3 className="mt-4 text-lg font-semibold text-foreground">
              请求级明细与实时计费核销
            </h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              按 Token 精确计价，请求发出前锁定费率，中途改配不影响已发生的账单。
            </p>

            <div className="mt-6 space-y-2 font-mono text-[0.6875rem]">
              {recentLogs.map((log) => (
                <div
                  key={log.time}
                  className="rounded-xl border border-[var(--hairline)] bg-surface-sunken p-2.5 flex items-center justify-between"
                >
                  <div>
                    <div className="text-foreground font-medium">{log.model}</div>
                    <div className="text-muted-foreground text-[0.625rem]">
                      {log.time} · {log.tokens}
                    </div>
                  </div>
                  <div className="text-right">
                    <div className="text-primary font-medium">{log.cost}</div>
                    <div className="text-success text-[0.625rem]">{log.latency}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div className="mt-6 pt-4 border-t border-[var(--hairline)] flex items-center justify-between text-xs text-muted-foreground">
            <span>支持按月导出 CSV</span>
            <span className="font-mono text-[0.6875rem]">零隐藏猫腻</span>
          </div>
        </div>

        {/* Card 3: IR Protocol Engine */}
        <div className="rounded-2xl border border-[var(--hairline)] bg-surface-1 p-6 shadow-xs flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between">
              <span className="font-mono text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                双向无损协议
              </span>
              <Layers className="size-4 text-muted-foreground" />
            </div>
            <h3 className="mt-4 text-lg font-semibold text-foreground">
              Canonical IR 协议互转引擎
            </h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              任一协议进，任一协议出。内部以统一中间表示 (IR) 流转，完美兼容思考链与工具调用。
            </p>

            <div className="mt-6 rounded-xl border border-[var(--hairline)] bg-surface-sunken p-4 space-y-3 text-xs">
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground font-mono text-[0.6875rem]">CLIENT</span>
                <span className="rounded bg-surface-2 px-2 py-0.5 text-foreground font-mono text-[0.6875rem]">
                  OpenAI / Claude Code
                </span>
              </div>
              <div className="flex items-center justify-center">
                <div className="h-4 w-px bg-primary/40" />
              </div>
              <div className="rounded-lg border border-primary/30 bg-primary/10 p-2 text-center font-mono text-xs font-semibold text-primary">
                Canonical Intermediate Representation
              </div>
              <div className="flex items-center justify-center">
                <div className="h-4 w-px bg-primary/40" />
              </div>
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground font-mono text-[0.6875rem]">UPSTREAM</span>
                <span className="rounded bg-surface-2 px-2 py-0.5 text-foreground font-mono text-[0.6875rem]">
                  Any Target Model Provider
                </span>
              </div>
            </div>
          </div>

          <div className="mt-6 pt-4 border-t border-[var(--hairline)] flex items-center justify-between text-xs text-muted-foreground">
            <span>支持 Thinking 思考流</span>
            <span className="font-mono text-[0.6875rem]">双向原生流式</span>
          </div>
        </div>
      </div>
    </section>
  )
}
