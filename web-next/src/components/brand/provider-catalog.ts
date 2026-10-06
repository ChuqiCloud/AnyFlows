export type ProviderOption = {
  id: string
  name: string
  logo: string
  aliases?: readonly string[]
}


// Brand identity is independent of the channel's transport adapter.
export const providerCatalog: readonly ProviderOption[] = [
  { id: 'openai', name: 'OpenAI', logo: 'OpenAI' },
  { id: 'codex', name: 'OpenAI Codex', logo: 'OpenAI', aliases: ['codex', 'chatgpt codex'] },
  { id: 'codex_oauth', name: 'OpenAI Codex OAuth', logo: 'OpenAI', aliases: ['codex oauth'] },
  { id: 'anthropic', name: 'Anthropic', logo: 'Anthropic' },
  { id: 'google', name: 'Google Gemini', logo: 'Gemini', aliases: ['gemini'] },
  { id: 'deepseek', name: 'DeepSeek', logo: 'DeepSeek' },
  { id: 'qwen', name: 'Qwen', logo: 'Qwen', aliases: ['dashscope'] },
  { id: 'alibaba', name: 'Alibaba Cloud', logo: 'Alibaba' },
  { id: 'moonshot', name: 'Moonshot AI', logo: 'Moonshot' },
  { id: 'kimi', name: 'Kimi', logo: 'Kimi' },
  { id: 'zhipu', name: 'Zhipu AI', logo: 'Zhipu', aliases: ['zhipuai', 'zai', '智谱'] },
  { id: 'minimax', name: 'MiniMax', logo: 'MiniMax' },
  { id: 'bytedance', name: 'ByteDance', logo: 'ByteDance' },
  { id: 'doubao', name: 'Doubao', logo: 'Doubao', aliases: ['豆包'] },
  { id: 'tencent', name: 'Tencent', logo: 'Tencent' },
  { id: 'hunyuan', name: 'Hunyuan', logo: 'Hunyuan' },
  { id: 'baidu', name: 'Baidu', logo: 'Baidu' },
  { id: 'wenxin', name: 'Wenxin', logo: 'Wenxin', aliases: ['ernie'] },
  { id: 'spark', name: 'iFLYTEK Spark', logo: 'Spark', aliases: ['iflytek'] },
  { id: 'baichuan', name: 'Baichuan', logo: 'Baichuan' },
  { id: 'stepfun', name: 'StepFun', logo: 'Stepfun' },
  { id: 'xai', name: 'xAI', logo: 'xAI' },
  { id: 'meta', name: 'Meta', logo: 'Meta' },
  { id: 'mistral', name: 'Mistral AI', logo: 'Mistral' },
  { id: 'cohere', name: 'Cohere', logo: 'Cohere' },
  { id: 'jina', name: 'Jina AI', logo: 'Jina' },
  { id: 'perplexity', name: 'Perplexity', logo: 'Perplexity' },
  { id: 'groq', name: 'Groq', logo: 'Groq' },
  { id: 'together', name: 'Together AI', logo: 'Together' },
  { id: 'openrouter', name: 'OpenRouter', logo: 'OpenRouter' },
  { id: 'ollama', name: 'Ollama', logo: 'Ollama' },
  { id: 'amazon', name: 'Amazon', logo: 'AWS', aliases: ['aws'] },
  { id: 'bedrock', name: 'Amazon Bedrock', logo: 'AWS', aliases: ['amazon-bedrock', 'aws-bedrock'] },
  { id: 'vertex', name: 'Google Vertex AI', logo: 'Google', aliases: ['vertexai', 'google-vertex'] },
  { id: 'microsoft', name: 'Microsoft', logo: 'Microsoft' },
  { id: 'azure', name: 'Microsoft Azure', logo: 'Azure', aliases: ['azureopenai'] },
  { id: 'siliconflow', name: 'SiliconFlow', logo: 'SiliconCloud', aliases: ['siliconcloud', '硅基流动'] },
  { id: 'fireworks', name: 'Fireworks AI', logo: 'Fireworks' },
  { id: 'nvidia', name: 'NVIDIA', logo: 'NVIDIA', aliases: ['nim'] },
  { id: 'huggingface', name: 'Hugging Face', logo: 'HuggingFace' },
  { id: 'cerebras', name: 'Cerebras', logo: 'Cerebras' },
  { id: 'sambanova', name: 'SambaNova', logo: 'SambaNova' },
  { id: 'ai21', name: 'AI21 Labs', logo: 'AI21' },
]

export function normalizeProvider(value: string) {
  return value.trim().toLowerCase().replace(/[\s._-]/g, '')
}

export function findProviderOption(provider: string, options: readonly ProviderOption[]) {
  const normalized = normalizeProvider(provider)
  if (!normalized) return undefined
  const separated = provider.trim().toLowerCase().replace(/[\s._-]+/g, '-')
  return options
    .map((item) => ({ item, score: [item.id, item.name, ...(item.aliases ?? [])].reduce((best, value) => {
      const candidate = normalizeProvider(value)
      if (!candidate) return best
      if (candidate === normalized) return Math.max(best, candidate.length + 1000)
      const prefix = value.trim().toLowerCase().replace(/[\s._-]+/g, '-')
      return candidate.length >= 3 && separated.startsWith(`${prefix}-`) ? Math.max(best, candidate.length) : best
    }, 0) }))
    .filter(({ score }) => score > 0)
    .sort((left, right) => right.score - left.score)[0]?.item
}

export function findProvider(provider: string) {
  return findProviderOption(provider, providerCatalog)
}

export function providerDisplayName(provider: string) {
  return findProvider(provider)?.name ?? provider
}

export function filterProviders(query: string, options = providerCatalog) {
  const normalized = normalizeProvider(query)
  return options.filter((item) => [item.id, item.name, ...(item.aliases ?? [])]
    .some((value) => normalizeProvider(value).includes(normalized)))
}
