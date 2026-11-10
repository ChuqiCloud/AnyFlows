import assert from 'node:assert/strict'
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import ReactMarkdown from 'react-markdown'
import rehypeKatex from 'rehype-katex'
import remarkGfm from 'remark-gfm'
import remarkMath from 'remark-math'
import test from 'node:test'

function renderMarkdown(content) {
  return renderToStaticMarkup(React.createElement(
    ReactMarkdown,
    { remarkPlugins: [remarkGfm, remarkMath], rehypePlugins: [rehypeKatex] },
    content,
  ))
}

test('KaTeX renders inline and display math through the playground Markdown pipeline', () => {
  const html = renderMarkdown('Inline $x^2$\n\n$$\ny = mx + b\n$$')
  assert.match(html, /class="katex"/)
  assert.match(html, /class="katex-display"/)
})

test('KaTeX reports malformed expressions without aborting the Markdown render', () => {
  const html = renderMarkdown('$\\frac{1}{2$')
  assert.match(html, /katex-error/)
})

test('inherited trust settings do not turn a formula link into a trusted link', () => {
  Object.defineProperty(Object.prototype, 'trust', {
    configurable: true,
    value: true,
  })
  try {
    const html = renderMarkdown('$\\href{https://tracking.invalid/path}{open}$')
    assert.doesNotMatch(html, /<a\b[^>]*href=/i)
  } finally {
    delete Object.prototype.trust
  }
})
