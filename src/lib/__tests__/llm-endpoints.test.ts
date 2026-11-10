import { describe, expect, it } from 'vitest'
import {
  chatModelsOnly,
  llmLogoForModel,
  llmProviderFromBaseUrl,
  llmServiceName,
} from '../llm-endpoints'

describe('llmProviderFromBaseUrl', () => {
  it.each([
    ['https://api.groq.com/openai/v1', 'groq'],
    ['https://api.openai.com/v1', 'openai'],
    ['https://api.anthropic.com/v1', 'claude'],
    ['https://openrouter.ai/api/v1', 'openrouter'],
    ['https://generativelanguage.googleapis.com/v1beta/openai', 'gemini'],
    ['http://localhost:11434/v1', 'ollama'],
    ['http://127.0.0.1:11434/v1', 'ollama'],
    ['https://dashscope-intl.aliyuncs.com/compatible-mode/v1', 'qwen'],
    ['https://ark.cn-shanghai.volces.com/api/v3', 'doubao'],
    ['https://api.z.ai/api/anthropic/v1', 'zhipu'],
    ['https://api.z.ai/api/paas/v4', 'zhipu'],
  ])('recognizes %s', (url, provider) => {
    expect(llmProviderFromBaseUrl(url)).toBe(provider)
  })

  it.each([
    'https://proxy.example.com/v1',
    'http://localhost:1234/v1',
    'https://api.groq.com.evil.example/v1',
    'not a url',
    '',
  ])('treats %s as a custom endpoint', (url) => {
    expect(llmProviderFromBaseUrl(url)).toBe('custom')
  })

  it('tolerates spaces and a missing path', () => {
    expect(llmProviderFromBaseUrl('  https://api.groq.com  ')).toBe('groq')
  })
})

describe('llmLogoForModel', () => {
  it.each([
    ['qwen/qwen3.8-27b', 'qwen'],
    ['Qwen/Qwen2.5-7B-Instruct', 'qwen'],
    ['gpt-4o-mini', 'openai'],
    ['openai/gpt-oss-120b', 'openai'],
    ['o3-mini', 'openai'],
    ['claude-sonnet-4-0', 'claude'],
    ['anthropic/claude-opus-4', 'claude'],
    ['google/gemini-2.5-flash', 'gemini'],
    ['deepseek-chat', 'deepseek'],
    ['meta-llama/llama-4-scout', 'meta'],
    ['llama-3.3-70b-versatile', 'meta'],
    ['glm-4-flash', 'zhipu'],
    ['moonshotai/kimi-k2', 'moonshot'],
    ['moonshot-v1-8k', 'moonshot'],
    ['doubao-seed-1-6-flash-250615', 'doubao'],
    ['mistralai/mistral-large', 'mistral'],
    ['grok-4', 'grok'],
  ])('maps %s to %s', (model, logo) => {
    expect(llmLogoForModel(model)).toBe(logo)
  })

  it('has no logo for an unknown model', () => {
    expect(llmLogoForModel('ep-20250101-abcde')).toBeNull()
  })
})

describe('chatModelsOnly', () => {
  it('drops speech, safety and embedding models and keeps the rest in order', () => {
    expect(
      chatModelsOnly([
        'qwen/qwen3.8-27b',
        'whisper-large-v3-turbo',
        'meta-llama/llama-guard-4-12b',
        'playai-tts',
        'text-embedding-3-small',
        'distil-whisper-large-v3-en',
        'llama-3.3-70b-versatile',
      ]),
    ).toEqual(['qwen/qwen3.8-27b', 'llama-3.3-70b-versatile'])
  })
})

describe('llmServiceName', () => {
  it('names a listed service by its id', () => {
    expect(llmServiceName('groq', 'https://api.groq.com/openai/v1')).toBe('groq')
  })

  it('names a custom endpoint by its host', () => {
    expect(llmServiceName('custom', 'https://proxy.example.com:8443/v1')).toBe(
      'proxy.example.com:8443',
    )
  })

  it('falls back to the id when a custom URL cannot be parsed', () => {
    expect(llmServiceName('custom', 'not a url')).toBe('custom')
  })
})
