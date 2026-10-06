import ReactMarkdown from 'react-markdown'
import rehypeKatex from 'rehype-katex'
import remarkGfm from 'remark-gfm'
import remarkMath from 'remark-math'

import 'katex/dist/katex.min.css'

import { PlaygroundCodeBlock } from './playground-code-block'

type PlaygroundMarkdownProps = {
  content: string
  streaming: boolean
}

export function PlaygroundMarkdown({ content, streaming }: PlaygroundMarkdownProps) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm, remarkMath]}
      rehypePlugins={[rehypeKatex]}
      components={{
        a: ({ children, href }) => (
          <a className="text-info underline underline-offset-4" href={href} rel="noreferrer" target="_blank">
            {children}
          </a>
        ),
        blockquote: ({ children }) => (
          <blockquote className="my-3 border-l-2 border-info/40 pl-3 text-muted-foreground">{children}</blockquote>
        ),
        code: ({ children, className }) => {
          const code = String(children).replace(/\n$/, '')
          const language = /language-([^\s]+)/.exec(className || '')?.[1]
          if (language || code.includes('\n')) {
            return <PlaygroundCodeBlock code={code} language={language} streaming={streaming} />
          }
          return <code className="rounded-md bg-surface-2 px-1 py-0.5 text-[0.9em]">{children}</code>
        },
        h1: ({ children }) => <h1 className="mb-2 mt-4 text-lg font-semibold first:mt-0">{children}</h1>,
        h2: ({ children }) => <h2 className="mb-2 mt-4 text-base font-semibold first:mt-0">{children}</h2>,
        h3: ({ children }) => <h3 className="mb-1.5 mt-3 text-sm font-semibold first:mt-0">{children}</h3>,
        li: ({ children }) => <li className="my-1">{children}</li>,
        ol: ({ children }) => <ol className="my-3 list-decimal space-y-1 pl-5">{children}</ol>,
        p: ({ children }) => <p className="my-2 first:mt-0 last:mb-0">{children}</p>,
        pre: ({ children }) => <>{children}</>,
        table: ({ children }) => (
          <div className="my-3 overflow-x-auto rounded-xl border border-[var(--hairline)]">
            <table className="w-full min-w-96 border-collapse text-left text-xs">{children}</table>
          </div>
        ),
        td: ({ children }) => <td className="border-t border-[var(--hairline)] px-3 py-2 align-top">{children}</td>,
        th: ({ children }) => <th className="bg-surface-2/70 px-3 py-2 font-medium">{children}</th>,
        ul: ({ children }) => <ul className="my-3 list-disc space-y-1 pl-5">{children}</ul>,
      }}
    >
      {content}
    </ReactMarkdown>
  )
}
