import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { LlmConnectionForm } from '../LlmConnectionForm'
import { LLM_ENDPOINTS } from '../../../lib/llm-endpoints'
import {
  benchLlmConnection,
  fetchLlmModels,
  readCredential,
  setCredential,
} from '../../../lib/tauri'

const baseConfig = {
  llm_provider: 'groq',
  llm_base_url: 'https://api.groq.com/openai/v1',
  llm_model: 'qwen/qwen3.8-27b',
  llm_api_key: '',
  llm_api_format: 'chat_completions',
  llm_fallback_model: '',
}

const mockStore = {
  config: { ...baseConfig },
  updateConfig: vi.fn(),
  llmTestStatus: 'idle',
  setLlmTestStatus: vi.fn(),
  llmLatencyMs: null as number | null,
  setLlmLatencyMs: vi.fn(),
  llmModels: [] as string[],
  setLlmModels: vi.fn(),
}

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: { count?: number }) =>
      values?.count === undefined ? key : `${key}:${values.count}`,
  }),
}))

vi.mock('../../../stores/appStore', () => ({
  useAppStore: (selector: (state: typeof mockStore) => unknown) => selector(mockStore),
}))

vi.mock('../../../lib/tauri', () => ({
  benchLlmConnection: vi.fn(),
  fetchLlmModels: vi.fn(),
  readCredential: vi.fn().mockResolvedValue('stored-key'),
  setCredential: vi.fn().mockResolvedValue(undefined),
}))

const openModelList = () => fireEvent.click(screen.getByRole('button', { name: /settings.model$/ }))

describe('LlmConnectionForm', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockStore.config = { ...baseConfig }
    mockStore.llmTestStatus = 'idle'
    mockStore.llmLatencyMs = null
    mockStore.llmModels = []
    vi.mocked(fetchLlmModels).mockResolvedValue([])
    vi.mocked(benchLlmConnection).mockResolvedValue({ latencyMs: 120, reasoningActive: false })
  })

  afterEach(cleanup)

  it('asks for the base URL, then the key, then the model, with no provider field', () => {
    const { container } = render(<LlmConnectionForm />)
    const labels = Array.from(container.querySelectorAll('[data-field]')).map((node) =>
      node.getAttribute('data-field'),
    )
    expect(labels).toEqual(['base-url', 'api-format', 'api-key', 'model', 'fallback-model'])
    expect(screen.queryByText('settings.provider')).toBeNull()
  })

  it('saves the fallback model as typed and lets it be cleared', () => {
    mockStore.config = { ...baseConfig, llm_fallback_model: 'spare' }
    render(<LlmConnectionForm />)
    const field = screen.getByLabelText('settings.fallbackModel')
    expect((field as HTMLInputElement).value).toBe('spare')
    fireEvent.change(field, { target: { value: '' } })
    expect(mockStore.updateConfig).toHaveBeenCalledWith({ llm_fallback_model: '' })
  })

  it('fills the URL, provider and default model when a listed service is picked', () => {
    render(<LlmConnectionForm />)
    fireEvent.focus(screen.getByLabelText('settings.baseUrl'))
    fireEvent.change(screen.getByLabelText('settings.baseUrl'), { target: { value: 'deep' } })
    fireEvent.click(screen.getByRole('option', { name: /providers.llm.deepseek/ }))
    expect(mockStore.updateConfig).toHaveBeenCalledWith({
      llm_provider: 'deepseek',
      llm_base_url: 'https://api.deepseek.com/v1',
      llm_model: 'deepseek-chat',
      llm_api_format: 'chat_completions',
    })
    expect(mockStore.setLlmModels).toHaveBeenCalledWith([])
  })

  it('opens every service from the arrow when the URL is not a listed one', () => {
    mockStore.config = { ...baseConfig, llm_base_url: 'https://api.z.ai/api/anthropic/v1' }
    render(<LlmConnectionForm />)
    fireEvent.click(screen.getByRole('button', { name: 'settings.llmChooseService' }))
    expect(screen.getAllByRole('option')).toHaveLength(LLM_ENDPOINTS.length)
  })

  it('opens every service when the field with an unlisted URL is focused', () => {
    mockStore.config = { ...baseConfig, llm_base_url: 'https://api.z.ai/api/anthropic/v1' }
    render(<LlmConnectionForm />)
    fireEvent.focus(screen.getByLabelText('settings.baseUrl'))
    expect(screen.getAllByRole('option')).toHaveLength(LLM_ENDPOINTS.length)
  })

  it('narrows the services to the typed text while typing', () => {
    mockStore.config = { ...baseConfig, llm_base_url: 'dee' }
    render(<LlmConnectionForm />)
    fireEvent.change(screen.getByLabelText('settings.baseUrl'), { target: { value: 'deep' } })
    expect(screen.getAllByRole('option').map((option) => option.textContent)).toEqual([
      expect.stringContaining('providers.llm.deepseek'),
    ])
  })

  it('derives the provider from a typed URL and keeps the model', () => {
    render(<LlmConnectionForm />)
    fireEvent.change(screen.getByLabelText('settings.baseUrl'), {
      target: { value: 'https://proxy.example.com/v1' },
    })
    expect(mockStore.updateConfig).toHaveBeenCalledWith({
      llm_provider: 'custom',
      llm_base_url: 'https://proxy.example.com/v1',
      llm_api_format: 'chat_completions',
    })
  })

  it('stores the key under the provider the URL resolves to', async () => {
    render(<LlmConnectionForm />)
    const input = screen.getByLabelText('settings.apiKey')
    fireEvent.change(input, { target: { value: 'new-key' } })
    fireEvent.blur(input)
    await waitFor(() => expect(setCredential).toHaveBeenCalledWith('llm', 'groq', 'new-key'))
  })

  it('test fetches the models, shows them and then measures the selected model', async () => {
    vi.mocked(fetchLlmModels).mockResolvedValue(['qwen/qwen3.8-27b', 'whisper-large-v3'])
    render(<LlmConnectionForm />)
    await screen.findByDisplayValue('stored-key')
    vi.mocked(fetchLlmModels).mockClear()

    fireEvent.click(screen.getByRole('button', { name: 'settings.test' }))

    await waitFor(() => expect(mockStore.setLlmTestStatus).toHaveBeenCalledWith('success'))
    expect(fetchLlmModels).toHaveBeenCalledWith(
      'stored-key',
      'groq',
      'https://api.groq.com/openai/v1',
      'chat_completions',
    )
    expect(mockStore.setLlmModels).toHaveBeenCalledWith(['qwen/qwen3.8-27b'])
    expect(benchLlmConnection).toHaveBeenCalledWith(
      'stored-key',
      'groq',
      'https://api.groq.com/openai/v1',
      'qwen/qwen3.8-27b',
      'chat_completions',
    )
    expect(mockStore.setLlmLatencyMs).toHaveBeenCalledWith(120)
  })

  it('test fills an empty model with the service default when it is listed', async () => {
    mockStore.config = { ...baseConfig, llm_model: '' }
    vi.mocked(fetchLlmModels).mockResolvedValue(['allam-2-7b', 'llama-3.3-70b-versatile'])
    render(<LlmConnectionForm />)
    await screen.findByDisplayValue('stored-key')

    fireEvent.click(screen.getByRole('button', { name: 'settings.test' }))

    await waitFor(() => expect(mockStore.setLlmTestStatus).toHaveBeenCalledWith('success'))
    expect(mockStore.updateConfig).toHaveBeenCalledWith({ llm_model: 'llama-3.3-70b-versatile' })
    expect(benchLlmConnection).toHaveBeenCalledWith(
      'stored-key',
      'groq',
      'https://api.groq.com/openai/v1',
      'llama-3.3-70b-versatile',
      'chat_completions',
    )
  })

  it('test with no model to measure succeeds and leaves the choice to the user', async () => {
    mockStore.config = { ...baseConfig, llm_model: '' }
    vi.mocked(fetchLlmModels).mockResolvedValue(['allam-2-7b'])
    render(<LlmConnectionForm />)
    await screen.findByDisplayValue('stored-key')

    fireEvent.click(screen.getByRole('button', { name: 'settings.test' }))

    await waitFor(() => expect(mockStore.setLlmTestStatus).toHaveBeenCalledWith('success'))
    expect(benchLlmConnection).not.toHaveBeenCalled()
    expect(mockStore.updateConfig).not.toHaveBeenCalledWith(
      expect.objectContaining({ llm_model: expect.anything() }),
    )
  })

  it('test still measures a typed model on a service without a model list', async () => {
    mockStore.config = { ...baseConfig, llm_model: 'ep-20250101-abcde' }
    vi.mocked(fetchLlmModels).mockRejectedValue('HTTP 404')
    render(<LlmConnectionForm />)
    await screen.findByDisplayValue('stored-key')

    fireEvent.click(screen.getByRole('button', { name: 'settings.test' }))

    await waitFor(() => expect(mockStore.setLlmTestStatus).toHaveBeenCalledWith('success'))
    expect(benchLlmConnection).toHaveBeenCalled()
  })

  it('test reports the fetch error when there is no model to fall back on', async () => {
    mockStore.config = { ...baseConfig, llm_model: '' }
    vi.mocked(fetchLlmModels).mockRejectedValue('HTTP 401: invalid key')
    render(<LlmConnectionForm />)
    await screen.findByDisplayValue('stored-key')

    fireEvent.click(screen.getByRole('button', { name: 'settings.test' }))

    expect(await screen.findByText('HTTP 401: invalid key')).toBeTruthy()
    expect(mockStore.setLlmTestStatus).toHaveBeenCalledWith('error')
    expect(benchLlmConnection).not.toHaveBeenCalled()
  })

  it('lists models with a logo and picks one', () => {
    mockStore.llmModels = ['qwen/qwen3.8-27b', 'llama-3.3-70b-versatile', 'ep-unknown']
    render(<LlmConnectionForm />)
    openModelList()

    const options = screen.getAllByRole('option')
    expect(options.map((option) => option.textContent)).toEqual(mockStore.llmModels)
    expect(options[1].querySelector('img')).not.toBeNull()
    expect(options[2].querySelector('img')).toBeNull()

    fireEvent.click(options[1])
    expect(mockStore.updateConfig).toHaveBeenCalledWith({ llm_model: 'llama-3.3-70b-versatile' })
  })

  it('times every model when the list is opened and shows the time beside it', async () => {
    mockStore.llmModels = ['qwen/qwen3.8-27b', 'llama-3.3-70b-versatile', 'ep-unknown']
    vi.mocked(benchLlmConnection).mockImplementation(async (_key, _provider, _url, model) => {
      if (model === 'ep-unknown') throw new Error('HTTP 404')
      return { latencyMs: model.startsWith('qwen') ? 180 : 420, reasoningActive: false }
    })
    render(<LlmConnectionForm />)
    await waitFor(() => expect(readCredential).toHaveBeenCalled())
    expect(benchLlmConnection).not.toHaveBeenCalled()

    openModelList()

    await waitFor(() =>
      expect(screen.getAllByRole('option').map((option) => option.textContent)).toEqual([
        'qwen/qwen3.8-27b180ms',
        'llama-3.3-70b-versatile420ms',
        'ep-unknown—',
      ]),
    )
    expect(benchLlmConnection).toHaveBeenCalledTimes(3)
  })

  it('puts the models in order of their times once all are timed, and marks the quickest', async () => {
    mockStore.llmModels = ['a-slow', 'b-dead', 'c-quick']
    vi.mocked(benchLlmConnection).mockImplementation(async (_key, _provider, _url, model) => {
      if (model === 'b-dead') throw new Error('HTTP 404')
      return { latencyMs: model === 'c-quick' ? 90 : 700, reasoningActive: false }
    })
    render(<LlmConnectionForm />)
    await waitFor(() => expect(readCredential).toHaveBeenCalled())
    openModelList()

    await waitFor(() =>
      expect(screen.getAllByRole('option').map((option) => option.textContent)).toEqual([
        'c-quick90ms',
        'a-slow700ms',
        'b-dead—',
      ]),
    )
    expect(screen.getByText('90ms').dataset.quickest).toBe('true')
    expect(screen.getByText('700ms').dataset.quickest).toBeUndefined()
  })

  it('does not time a model again when the list is opened a second time', async () => {
    mockStore.llmModels = ['qwen/qwen3.8-27b', 'llama-3.3-70b-versatile']
    render(<LlmConnectionForm />)
    await waitFor(() => expect(readCredential).toHaveBeenCalled())
    openModelList()
    await waitFor(() => expect(screen.getAllByText('120ms')).toHaveLength(2))

    openModelList()
    openModelList()

    expect(screen.getAllByText('120ms')).toHaveLength(2)
    expect(benchLlmConnection).toHaveBeenCalledTimes(2)
  })

  it('filters the model list and accepts a typed name that is not listed', () => {
    mockStore.llmModels = ['qwen/qwen3.8-27b', 'llama-3.3-70b-versatile']
    render(<LlmConnectionForm />)
    openModelList()

    const search = screen.getByPlaceholderText('settings.modelSearchPlaceholder')
    fireEvent.change(search, { target: { value: 'llama' } })
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual([
      'llama-3.3-70b-versatile',
    ])

    fireEvent.change(search, { target: { value: 'ep-custom-1' } })
    fireEvent.keyDown(search, { key: 'Enter' })
    expect(mockStore.updateConfig).toHaveBeenCalledWith({ llm_model: 'ep-custom-1' })
  })

  it('shows the key from the vault rather than a stale one in the config', async () => {
    mockStore.config = { ...baseConfig, llm_api_key: 'old-key' }
    render(<LlmConnectionForm />)
    expect(await screen.findByDisplayValue('stored-key')).toBeTruthy()
  })

  it('shows an inline error when the vault write fails', async () => {
    vi.mocked(setCredential).mockRejectedValueOnce(new Error('vault locked'))
    render(<LlmConnectionForm />)
    const input = screen.getByLabelText('settings.apiKey')
    fireEvent.change(input, { target: { value: 'new-key' } })
    fireEvent.blur(input)
    expect(await screen.findByText('settings.credentialSaveFailed')).toBeTruthy()
  })

  it.each([
    ['ollama', 'http://localhost:11434/v1'],
    ['custom', 'http://localhost:1234/v1'],
  ])('can test %s without a key', async (provider, url) => {
    vi.mocked(readCredential).mockResolvedValueOnce(null)
    mockStore.config = { ...baseConfig, llm_provider: provider, llm_base_url: url }
    render(<LlmConnectionForm />)
    const test = screen.getByRole('button', { name: 'settings.test' }) as HTMLButtonElement
    expect(test.disabled).toBe(false)
    fireEvent.click(test)
    await waitFor(() =>
      expect(fetchLlmModels).toHaveBeenCalledWith('', provider, url, 'chat_completions'),
    )
  })

  it('fetches models by itself once a keyless service is reachable', async () => {
    vi.mocked(readCredential).mockResolvedValueOnce(null)
    vi.mocked(fetchLlmModels).mockResolvedValue(['llama3.2'])
    mockStore.config = {
      ...baseConfig,
      llm_provider: 'ollama',
      llm_base_url: 'http://localhost:11434/v1',
    }
    render(<LlmConnectionForm />)
    await waitFor(() => expect(mockStore.setLlmModels).toHaveBeenCalledWith(['llama3.2']), {
      timeout: 1500,
    })
  })

  it('does not fetch models or allow a test before a required key is entered', async () => {
    vi.mocked(readCredential).mockResolvedValueOnce(null)
    render(<LlmConnectionForm />)
    await new Promise((resolve) => setTimeout(resolve, 600))
    expect(fetchLlmModels).not.toHaveBeenCalled()
    expect(
      (screen.getByRole('button', { name: 'settings.test' }) as HTMLButtonElement).disabled,
    ).toBe(true)
  })

  it('shows the service error when the model test fails', async () => {
    vi.mocked(benchLlmConnection).mockRejectedValue('HTTP 401: invalid key')
    render(<LlmConnectionForm />)
    await screen.findByDisplayValue('stored-key')
    fireEvent.click(screen.getByRole('button', { name: 'settings.test' }))
    expect(await screen.findByText('HTTP 401: invalid key')).toBeTruthy()
    expect(mockStore.setLlmTestStatus).toHaveBeenCalledWith('error')
  })

  it('shows the latency after a successful test', () => {
    mockStore.llmTestStatus = 'success'
    mockStore.llmLatencyMs = 342
    render(<LlmConnectionForm />)
    expect(screen.getByText('342ms')).toBeTruthy()
  })

  it('offers the three interface formats and saves the choice', () => {
    render(<LlmConnectionForm />)
    const radios = screen.getAllByRole('radio')
    expect(radios.map((radio) => radio.getAttribute('aria-label'))).toEqual([
      'OpenAI Chat Completions',
      'OpenAI Responses',
      'Anthropic Messages',
    ])
    expect(radios.map((radio) => radio.textContent)).toEqual([
      'Chat Completions',
      'Responses',
      'Messages',
    ])
    expect(radios[0].getAttribute('aria-checked')).toBe('true')

    fireEvent.click(radios[1])
    expect(mockStore.updateConfig).toHaveBeenCalledWith({ llm_api_format: 'responses' })
    expect(mockStore.setLlmModels).toHaveBeenCalledWith([])
  })

  it('shows the format a saved config without one will use', () => {
    mockStore.config = {
      ...baseConfig,
      llm_provider: 'claude',
      llm_base_url: 'https://api.anthropic.com/v1',
      llm_api_format: '',
    }
    render(<LlmConnectionForm />)
    expect(
      screen.getByRole('radio', { name: 'Anthropic Messages' }).getAttribute('aria-checked'),
    ).toBe('true')
  })

  it('selects Anthropic Messages when Claude is picked', () => {
    render(<LlmConnectionForm />)
    fireEvent.focus(screen.getByLabelText('settings.baseUrl'))
    fireEvent.change(screen.getByLabelText('settings.baseUrl'), { target: { value: 'anthropic' } })
    fireEvent.click(screen.getByRole('option', { name: /providers.llm.claude/ }))
    expect(mockStore.updateConfig).toHaveBeenCalledWith(
      expect.objectContaining({ llm_api_format: 'anthropic_messages' }),
    )
  })

  it('keeps the chosen format while a custom URL is edited', () => {
    mockStore.config = {
      ...baseConfig,
      llm_provider: 'custom',
      llm_base_url: 'http://gateway.example.com/v',
      llm_api_format: 'responses',
    }
    render(<LlmConnectionForm />)
    fireEvent.change(screen.getByLabelText('settings.baseUrl'), {
      target: { value: 'http://gateway.example.com/v1' },
    })
    expect(mockStore.updateConfig).toHaveBeenCalledWith({
      llm_provider: 'custom',
      llm_base_url: 'http://gateway.example.com/v1',
      llm_api_format: 'responses',
    })
  })

  it('sends the chosen format with the test', async () => {
    mockStore.config = { ...baseConfig, llm_api_format: 'responses' }
    render(<LlmConnectionForm />)
    await screen.findByDisplayValue('stored-key')
    fireEvent.click(screen.getByRole('button', { name: 'settings.test' }))
    await waitFor(() => expect(mockStore.setLlmTestStatus).toHaveBeenCalledWith('success'))
    expect(fetchLlmModels).toHaveBeenCalledWith(
      'stored-key',
      'groq',
      'https://api.groq.com/openai/v1',
      'responses',
    )
    expect(benchLlmConnection).toHaveBeenCalledWith(
      'stored-key',
      'groq',
      'https://api.groq.com/openai/v1',
      'qwen/qwen3.8-27b',
      'responses',
    )
  })

  it('warns when the test shows the model is still reasoning', async () => {
    vi.mocked(benchLlmConnection).mockResolvedValue({ latencyMs: 900, reasoningActive: true })
    render(<LlmConnectionForm />)
    await screen.findByDisplayValue('stored-key')
    fireEvent.click(screen.getByRole('button', { name: 'settings.test' }))
    expect(await screen.findByRole('alert')).toHaveProperty(
      'textContent',
      'settings.llmReasoningActive',
    )
  })

  it('shows no warning when reasoning is off', async () => {
    render(<LlmConnectionForm />)
    await screen.findByDisplayValue('stored-key')
    fireEvent.click(screen.getByRole('button', { name: 'settings.test' }))
    await waitFor(() => expect(mockStore.setLlmLatencyMs).toHaveBeenCalledWith(120))
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('closes the model list on Escape', () => {
    mockStore.llmModels = ['qwen/qwen3.8-27b']
    render(<LlmConnectionForm />)
    openModelList()
    fireEvent.keyDown(screen.getByPlaceholderText('settings.modelSearchPlaceholder'), {
      key: 'Escape',
    })
    expect(screen.queryByRole('option')).toBeNull()
  })
})
