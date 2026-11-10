import type { LlmApiFormat, LlmProvider } from '../stores/appStore'
import { LLM_DEFAULT_CONFIG, LLM_PROVIDERS } from './constants'

export const CUSTOM_LLM_PROVIDER = 'custom' as const

export interface LlmEndpoint {
  provider: LlmProvider
  labelKey: string
  baseUrl: string
  model: string
}

export const LLM_ENDPOINTS: LlmEndpoint[] = LLM_PROVIDERS.map((entry) => ({
  provider: entry.value as LlmProvider,
  labelKey: entry.labelKey,
  ...LLM_DEFAULT_CONFIG[entry.value],
}))

// Hosts beyond the one in each default base URL. A subdomain of a listed host also matches.
const EXTRA_HOSTS: Partial<Record<LlmProvider, string[]>> = {
  qwen: ['dashscope-intl.aliyuncs.com'],
  doubao: ['volces.com'],
  zhipu: ['z.ai'],
  ollama: ['127.0.0.1:11434'],
}

export const LLM_API_FORMATS: {
  value: LlmApiFormat
  label: string
  short: string
  logo: string
}[] = [
  {
    value: 'chat_completions',
    label: 'OpenAI Chat Completions',
    short: 'Chat Completions',
    logo: 'openai',
  },
  { value: 'responses', label: 'OpenAI Responses', short: 'Responses', logo: 'openai' },
  { value: 'anthropic_messages', label: 'Anthropic Messages', short: 'Messages', logo: 'claude' },
]

export function llmDefaultApiFormat(provider: LlmProvider): LlmApiFormat {
  return provider === 'claude' ? 'anthropic_messages' : 'chat_completions'
}

function hostOf(url: string): string | null {
  try {
    return new URL(url.trim()).host.toLowerCase()
  } catch {
    return null
  }
}

function hostMatches(host: string, listed: string): boolean {
  return host === listed || host.endsWith(`.${listed}`)
}

/** The service a base URL belongs to, which picks its key slot and logo. */
export function llmProviderFromBaseUrl(baseUrl: string): LlmProvider {
  const host = hostOf(baseUrl)
  if (!host) return CUSTOM_LLM_PROVIDER
  const match = LLM_ENDPOINTS.find((endpoint) => {
    const listed = [hostOf(endpoint.baseUrl), ...(EXTRA_HOSTS[endpoint.provider] ?? [])]
    return listed.some((candidate) => candidate !== null && hostMatches(host, candidate))
  })
  return match?.provider ?? CUSTOM_LLM_PROVIDER
}

export function llmServiceName(provider: LlmProvider, baseUrl: string): string {
  return provider === CUSTOM_LLM_PROVIDER ? (hostOf(baseUrl) ?? provider) : provider
}

const MODEL_LOGOS: [RegExp, string][] = [
  [/qwen/i, 'qwen'],
  [/claude|anthropic/i, 'claude'],
  [/gemini|gemma|google\//i, 'gemini'],
  [/deepseek/i, 'deepseek'],
  [/openai|(^|\/)(chatgpt|gpt|o[134](-|$))/i, 'openai'],
  [/llama|meta-/i, 'meta'],
  [/glm|zhipu|z-ai/i, 'zhipu'],
  [/kimi|moonshot/i, 'moonshot'],
  [/doubao|bytedance/i, 'doubao'],
  [/mistral|mixtral|codestral/i, 'mistral'],
  [/grok|x-ai/i, 'grok'],
  [/groq/i, 'groq'],
  [/cohere|(^|\/)command-/i, 'cohere'],
  [/nvidia|nemotron/i, 'nvidia'],
  [/minimax/i, 'minimax'],
  [/perplexity|(^|\/)sonar/i, 'perplexity'],
]

export function llmLogoForModel(model: string): string | null {
  return MODEL_LOGOS.find(([pattern]) => pattern.test(model))?.[1] ?? null
}

const NON_CHAT_MODEL =
  /whisper|guard|embed|transcri|moderation|rerank|dall-e|image|audio|speech|(^|[-_/])tts($|[-_])/i

/** Model lists also name speech, safety and embedding models that cannot polish text. */
export function chatModelsOnly(models: string[]): string[] {
  return models.filter((model) => !NON_CHAT_MODEL.test(model))
}
