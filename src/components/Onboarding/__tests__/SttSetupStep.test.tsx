import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SttSetupStep } from '../SttSetupStep'
import { testSttConnection } from '../../../lib/tauri'

const baseConfig = {
  stt_provider: 'local-qwen3',
  stt_api_key: '',
  stt_custom_api_key: '',
  stt_custom_base_url: 'http://localhost:8000/v1',
  stt_custom_model: 'Systran/faster-whisper-large-v3',
}

const mockStore = {
  config: { ...baseConfig },
  updateConfig: vi.fn(),
  sttTestStatus: 'idle',
  setSttTestStatus: vi.fn(),
}

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

vi.mock('../../../stores/appStore', () => ({
  useAppStore: (selector: (state: typeof mockStore) => unknown) => selector(mockStore),
}))

vi.mock('../../Settings/LocalModelCard', () => ({
  LocalModelCard: () => <div>Local models</div>,
}))

vi.mock('../../../lib/tauri', () => ({
  testSttConnection: vi.fn(),
}))

describe('SttSetupStep', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockStore.config = { ...baseConfig }
    mockStore.sttTestStatus = 'idle'
  })

  afterEach(cleanup)

  it('opens on the local models for a new install', () => {
    render(<SttSetupStep />)
    expect(screen.getByText('Local models')).toBeTruthy()
    expect(screen.queryByText('onboarding.stt.apiKeyLabel')).toBeNull()
  })

  it('switches to a cloud API, picking the first listed service', () => {
    render(<SttSetupStep />)
    fireEvent.click(screen.getByRole('tab', { name: 'onboarding.stt.apiTab' }))
    expect(mockStore.updateConfig).toHaveBeenCalledWith({ stt_provider: 'deepgram' })
    expect(mockStore.setSttTestStatus).toHaveBeenCalledWith('idle')
  })

  it('opens on the API form when a cloud service is already configured', () => {
    mockStore.config = { ...baseConfig, stt_provider: 'groq-whisper' }
    render(<SttSetupStep />)
    expect(screen.queryByText('Local models')).toBeNull()
    expect(screen.getByRole('combobox').textContent).toContain('providers.stt.groqWhisper')
    expect(mockStore.updateConfig).not.toHaveBeenCalled()
  })

  it('switches back to local', () => {
    mockStore.config = { ...baseConfig, stt_provider: 'groq-whisper' }
    render(<SttSetupStep />)
    fireEvent.click(screen.getByRole('tab', { name: 'onboarding.stt.localTab' }))
    expect(mockStore.updateConfig).toHaveBeenCalledWith({ stt_provider: 'local-qwen3' })
  })

  it('keeps Apple Speech when it is the current provider, without asking for a key', () => {
    mockStore.config = { ...baseConfig, stt_provider: 'apple-speech' }
    render(<SttSetupStep />)
    expect(screen.getByRole('combobox').textContent).toContain('providers.stt.appleSpeech')
    expect(mockStore.updateConfig).not.toHaveBeenCalled()
    expect(screen.queryByText('onboarding.stt.apiKeyLabel')).toBeNull()
  })

  it('does not offer Apple Speech or local inside the API list otherwise', () => {
    mockStore.config = { ...baseConfig, stt_provider: 'groq-whisper' }
    render(<SttSetupStep />)
    fireEvent.click(screen.getByRole('combobox'))
    const options = screen.getAllByRole('option').map((option) => option.textContent)
    expect(options).toContain('providers.stt.groqWhisper')
    expect(options).not.toContain('providers.stt.appleSpeech')
    expect(options).not.toContain('providers.stt.localQwen3')
    expect(options).not.toContain('providers.stt.customWhisper')
  })

  it('preserves an existing Custom Whisper setup', () => {
    mockStore.config = { ...baseConfig, stt_provider: 'custom-whisper' }
    render(<SttSetupStep />)
    expect(screen.getByText('onboarding.stt.customWhisperConfigured')).toBeTruthy()
    expect(mockStore.updateConfig).not.toHaveBeenCalled()
  })

  it('tests Custom Whisper with its configured endpoint and model', async () => {
    vi.mocked(testSttConnection).mockResolvedValue(true)
    mockStore.config = { ...baseConfig, stt_provider: 'custom-whisper' }
    render(<SttSetupStep />)
    fireEvent.click(screen.getByRole('button', { name: 'onboarding.stt.testButton' }))
    await waitFor(() =>
      expect(testSttConnection).toHaveBeenCalledWith(
        '',
        'custom-whisper',
        'http://localhost:8000/v1',
        'Systran/faster-whisper-large-v3',
      ),
    )
    expect(mockStore.setSttTestStatus).toHaveBeenCalledWith('success')
  })
})
