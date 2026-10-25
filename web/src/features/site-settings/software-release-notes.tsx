import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'

type MarkdownNode = {
  type: string
  depth?: number
  value?: string
  children?: MarkdownNode[]
}

// Published releases append a dedicated checksum section after the changelog.
// Hide it only when the entire section contains checksum data, preserving prose.
function remarkReleaseNotes() {
  return (tree: unknown) => {
    const root = tree as { children?: MarkdownNode[] }
    if (!Array.isArray(root.children)) return
    const nodes = root.children
    const visible: MarkdownNode[] = []
    for (let index = 0; index < nodes.length; index += 1) {
      const node = nodes[index]
      if (node.type !== 'heading' || node.children?.map((child) => child.value || '').join('').trim().toUpperCase() !== 'SHA-256') {
        visible.push(node)
        continue
      }
      let end = index + 1
      while (end < nodes.length && !(nodes[end].type === 'heading' && (nodes[end].depth || 1) <= (node.depth || 1))) end += 1
      const section = nodes.slice(index + 1, end)
      const checksumOnly = section.length > 0 && section.every((entry) => {
        const value = entry.value?.trim()
        return entry.type === 'code' && value && value.split(/\r?\n/).every((line) => /^[a-f\d]{64}\s+\*?\S.*$/i.test(line))
      })
      if (checksumOnly) index = end - 1
      else visible.push(node)
    }
    root.children = visible
  }
}

export function SoftwareReleaseNotes({ content }: { content: string }) {
  return (
    <div className="min-w-0 text-sm leading-7 text-foreground/85 [overflow-wrap:anywhere]">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkReleaseNotes]}
        skipHtml
        components={{
          a: ({ children, href }) => <a href={href} target="_blank" rel="noopener noreferrer" className="font-medium text-info underline decoration-info/30 underline-offset-4 transition-colors hover:decoration-info focus-visible:rounded focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-ring">{children}</a>,
          blockquote: ({ children }) => <blockquote className="my-4 rounded-r-xl border-l-2 border-primary/30 bg-primary/5 px-4 py-1 text-muted-foreground">{children}</blockquote>,
          code: ({ children, className }) => <code className={`rounded-md bg-muted px-1.5 py-0.5 font-mono text-[0.85em] ${className || ''}`}>{children}</code>,
          h1: ({ children }) => <h4 className="mb-3 mt-7 text-lg font-semibold tracking-tight text-foreground first:mt-0">{children}</h4>,
          h2: ({ children }) => <h4 className="mb-3 mt-7 text-base font-semibold tracking-tight text-foreground first:mt-0">{children}</h4>,
          h3: ({ children }) => <h5 className="mb-2 mt-5 font-semibold text-foreground first:mt-0">{children}</h5>,
          h4: ({ children }) => <h6 className="mb-2 mt-4 font-semibold text-foreground first:mt-0">{children}</h6>,
          h5: ({ children }) => <h6 className="mb-2 mt-4 text-sm font-semibold text-foreground first:mt-0">{children}</h6>,
          h6: ({ children }) => <h6 className="mb-2 mt-4 text-sm font-medium text-foreground first:mt-0">{children}</h6>,
          hr: () => <hr className="my-6 border-[var(--hairline)]" />,
          img: ({ alt }) => alt ? <span className="text-muted-foreground">{alt}</span> : null,
          input: ({ checked }) => <input type="checkbox" checked={checked} disabled className="mr-2 size-3.5 align-middle accent-primary" />,
          li: ({ children, className }) => <li className={`my-1 pl-1 marker:text-muted-foreground ${className?.includes('task-list-item') ? 'list-none' : ''}`}>{children}</li>,
          ol: ({ children, start }) => <ol start={start} className="my-3 list-decimal space-y-1 pl-5">{children}</ol>,
          p: ({ children }) => <p className="my-3 first:mt-0 last:mb-0">{children}</p>,
          pre: ({ children }) => <pre className="my-4 max-w-full overflow-x-auto rounded-xl border border-[var(--hairline)] bg-muted/50 p-4 text-xs leading-6 [&_code]:break-normal [&_code]:bg-transparent [&_code]:p-0 [&_code]:text-inherit [&_code]:whitespace-pre [&_code]:[overflow-wrap:normal]">{children}</pre>,
          strong: ({ children }) => <strong className="font-semibold text-foreground">{children}</strong>,
          table: ({ children }) => <div className="my-4 max-w-full overflow-x-auto rounded-xl border border-[var(--hairline)]"><table className="w-full border-collapse text-left text-sm">{children}</table></div>,
          td: ({ children, style }) => <td style={style} className="min-w-28 border-t border-[var(--hairline)] px-4 py-3 align-top">{children}</td>,
          th: ({ children, style }) => <th style={style} className="min-w-28 bg-muted/60 px-4 py-3 font-semibold text-foreground">{children}</th>,
          ul: ({ children }) => <ul className="my-3 list-disc space-y-1 pl-5">{children}</ul>,
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  )
}
